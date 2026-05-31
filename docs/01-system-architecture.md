# 01 — System Architecture

## 1. Guiding constraints

The architecture is derived backwards from the targets in the [README](./README.md): **< 2 s start, < 500 MB idle, 1M+ files, 60 fps editing.** Three structural rules fall out of those numbers:

1. **The UI thread does nothing slow.** No parsing, no indexing, no disk walks, no network. The UI thread renders and dispatches intent. Everything expensive lives in Rust worker threads behind an async message bus.
2. **Everything heavy is incremental and lazy.** We never do an O(project) operation when an O(change) operation will do. We never compute what the user can't currently see.
3. **Memory is budgeted, not hoped for.** Each subsystem has a memory budget and a spill-to-disk strategy. The index is SQLite + memory-mapped caches, not a fully resident object graph.

## 2. Process & thread model

Photon runs as **one OS process** (the Tauri shell) hosting several logical zones, plus **out-of-process sandboxes** for untrusted work.

```
┌──────────────────────────────────────────────────────────────────────┐
│  Photon Process (Tauri)                                                │
│                                                                        │
│  ┌─────────────────────────┐        ┌───────────────────────────────┐ │
│  │  UI Zone (WebView)      │        │  Core Zone (Rust, Tokio)      │ │
│  │  React + TS + Tailwind  │  IPC   │                               │ │
│  │  - editor surface       │◄──────►│  - command bus / dispatcher   │ │
│  │  - panels, palette      │ async  │  - workspace service          │ │
│  │  - view-models only     │ events │  - indexer service            │ │
│  └─────────────────────────┘        │  - language host (LSP+PHP)    │ │
│         (no heavy logic)            │  - db tools service           │ │
│                                      │  - git service                │ │
│                                      │  - ai orchestrator            │ │
│                                      └───────────────┬───────────────┘ │
│                                                      │ supervised      │
│                                  ┌───────────────────┴───────────────┐ │
│                                  │  Worker pool (rayon + tokio tasks)│ │
│                                  │  parse / analyze / index shards   │ │
│                                  └───────────────────────────────────┘ │
└───────────────┬───────────────────────────────┬──────────────────────┘
                │ child process                  │ child process
   ┌────────────┴───────────┐      ┌─────────────┴────────────┐
   │ Plugin host (sandbox)  │      │ External LSP servers      │
   │ WASM / restricted node │      │ (phpactor/intelephense)*  │
   └────────────────────────┘      └───────────────────────────┘
```

\* External LSP servers are optional fallback engines; the primary PHP intelligence is Photon's own engine (see [05](./05-php-analysis-engine.md)). The architecture allows either or both.

### Thread zones

- **UI thread (WebView main):** input, layout, paint. Target: never blocked > 16 ms.
- **Core dispatcher (1 Tokio runtime):** owns the command bus, routes requests to services, fans events back to the UI. Never does CPU-bound work itself.
- **CPU pool (rayon):** parsing, semantic analysis, index shard builds. Sized to `min(cores, 8)` by default with a low-priority QoS class so the machine stays usable.
- **IO pool (tokio blocking):** file walks, SQLite, git plumbing, network.
- **Plugin host:** separate child process; cannot touch the UI thread or core memory directly (see [07](./07-plugin-sdk.md)).

## 3. Layered architecture

Photon is layered top-to-bottom; **dependencies only point downward.**

```
┌───────────────────────────────────────────────────────────┐
│ L5  Presentation      React view-models, panels, palette   │
├───────────────────────────────────────────────────────────┤
│ L4  Application       Commands, actions, orchestrators     │
│                       ("Rename symbol", "Run query")       │
├───────────────────────────────────────────────────────────┤
│ L3  Domain services   workspace, indexer, language host,   │
│                       navigation, refactoring, db, git, ai │
├───────────────────────────────────────────────────────────┤
│ L2  Intelligence core PHP engine, Laravel engine,          │
│                       tree-sitter, type system, resolver   │
├───────────────────────────────────────────────────────────┤
│ L1  Persistence       SQLite index, blob/content store,    │
│                       change log, caches, settings store   │
├───────────────────────────────────────────────────────────┤
│ L0  Platform          Tauri, OS FS/watcher, process mgmt,  │
│                       crypto/keychain, WebView             │
└───────────────────────────────────────────────────────────┘
```

The **module map** (`core/`, `editor/`, `workspace/`, `indexer/`, `navigation/`, `refactoring/`, `database/`, `terminal/`, `git/`, `debugger/`, `plugins/`, `laravel/`, `php/`, `ai/`, `settings/`, `ui/`) is detailed in [02 — Module Design](./02-module-design.md). Each module is a crate (Rust) or package (TS) with an explicit public interface, so it is independently replaceable.

## 4. The command bus (everything is a message)

All UI→Core interaction is a typed, versioned message. There is **no synchronous FFI** that can block the UI.

```rust
// Illustrative contract
pub enum Request {
    OpenFile { path: PathBuf, view: ViewId },
    EditDocument { doc: DocId, edits: Vec<TextEdit>, rev: u64 },
    GoToDefinition { doc: DocId, pos: Position },
    SearchEverywhere { query: String, scopes: SearchScopes, limit: u32 },
    RunQuery { conn: ConnId, sql: String },
    // ...
}

pub enum Event {
    Diagnostics { doc: DocId, items: Vec<Diagnostic> },
    SearchResults { query_id: u64, batch: Vec<SearchHit>, done: bool },
    IndexProgress { indexed: u64, total: u64 },
    // ...
}
```

Key properties:
- **Request/stream duality.** A request may produce one reply or a stream of events (e.g., Search Everywhere streams results so the first matches paint in < 100 ms while the rest arrive).
- **Cancellation is first-class.** Every long-running request carries a token; navigating away cancels stale work immediately. This is what keeps the IDE feeling instant under load.
- **Revisions everywhere.** Documents carry monotonic revisions so results from stale analysis are dropped, never rendered.

## 5. Editing & analysis data flow

```
keypress ─► UI applies edit optimistically (local rope)
         └─► EditDocument{rev} ─► Core
                                   ├─ apply to authoritative rope
                                   ├─ tree-sitter incremental re-parse (µs–ms)
                                   ├─ debounce ─► semantic analysis (CPU pool)
                                   └─ schedule index delta (IO pool)
                                          │
              Diagnostics / hints / semantic tokens ◄── stream back to UI
```

The editor never waits for analysis to show text. Syntax highlighting comes from tree-sitter near-instantly; semantic tokens, diagnostics, and inline hints stream in and patch the view as they're ready. This decoupling is what lets typing stay at 60 fps while a 1M-file project re-indexes in the background.

## 6. Startup sequence (how we hit < 2 s)

Startup is staged so the user is editing before the project is fully understood.

| Stage | Budget | What happens |
|---|---|---|
| **T0 paint** | < 250 ms | Tauri window + WebView shell + last layout (from settings store) painted. No project work yet. |
| **T1 shell ready** | < 600 ms | Recent project list, theme, keymap loaded. If reopening, last open files restored from session cache. |
| **T2 editable** | < 1.2 s | Open files parsed with tree-sitter; you can type, scroll, multi-cursor. Index is loaded read-only from the prior SQLite snapshot. |
| **T3 intelligent** | < 2 s (warm) | Symbol index validated against file mtimes; only changed files re-indexed. Go-to-definition, completion live. |
| **T4 deep** | background | Laravel intelligence, full project index, vendor analysis complete asynchronously with a progress affordance. |

The trick: **we persist the index between sessions** (see [03](./03-database-schema.md) and [04](./04-indexing-engine.md)), so warm starts skip almost all work. Cold first-open of a 1M-file repo shows an editable UI immediately and back-fills intelligence; it does not block on a full scan.

## 7. Memory strategy (how we stay < 500 MB)

- **Index lives in SQLite + memory-mapped pages**, not as a resident object graph. Hot symbols are cached with an LRU bounded by budget; cold symbols are a query away.
- **Documents are reference-counted ropes**; closed files release their syntax trees. Only open/visible buffers hold trees.
- **Tree-sitter trees are reused incrementally** rather than rebuilt; old trees are dropped on edit.
- **Per-subsystem budgets** with backpressure: the indexer, AI context engine, and DB result grids each have caps and spill/evict policies. Result grids virtualize and page rather than materializing whole result sets.
- **WebView heap is kept lean** by holding only view-models (visible viewport + small overscan), never the whole file model. The authoritative model is in Rust.

A medium project (Laravel app + vendor, ~50k PHP files) targets ~250–400 MB RSS at idle. The 500 MB ceiling holds because resident memory scales with *what's open and visible*, not with project size.

## 8. Failure isolation & resilience

- **Services are supervised.** If the language host panics, it is restarted; the UI shows a transient "intelligence reloading" state and editing continues.
- **Plugins are out-of-process and resource-capped** — a misbehaving plugin can be killed without touching the editor (see [07](./07-plugin-sdk.md)).
- **The index is crash-safe** via a write-ahead change log; a hard kill replays the log on next start rather than rebuilding.
- **Optimistic edits are journaled**, so a core crash mid-edit recovers unsaved buffers.

## 9. Cross-platform posture

One Rust core, one React UI, three targets (macOS, Windows, Linux). Platform specifics (file watching APIs, keychain, menu integration, WebView quirks) are isolated behind the L0 platform layer so the rest of the codebase is platform-agnostic. See [09](./09-tauri-rust-backend.md) for the crate-level detail.

## 10. Why this beats the JVM-IDE shape

JVM IDEs pay a large fixed cost: VM warmup, a resident object model of the whole project, and GC pressure proportional to that model. Photon avoids all three by (a) starting native with no VM warmup, (b) keeping the project model on disk and paging it, and (c) using Rust's deterministic memory management so there is no GC pause to interrupt typing. The result is the PhpStorm mental model with the VS Code resource envelope — which is the entire product thesis.

→ Next: [02 — Domain-Driven Module Design](./02-module-design.md)
