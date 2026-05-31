# 09 — Tauri / Rust Backend Architecture

This is the engine room. It explains why Tauri over Electron, the Rust crate layout, the IPC boundary, the async runtime model, and the concrete memory tactics that keep idle RSS under 500 MB.

## 1. Why Tauri (and explicitly not Electron)

| Concern | Electron | Tauri (chosen) |
|---|---|---|
| Renderer | Bundles Chromium (~150–200 MB baseline) | Uses the OS WebView (WKWebView/WebView2/WebKitGTK) — near-zero baseline |
| Backend language | Node.js | **Rust** — no GC pauses, deterministic memory, native speed |
| Bundle size | 100s of MB | 5–15 MB typical |
| Idle memory | High fixed floor | Low; scales with actual use |
| Startup | VM + Chromium init | Native window + WebView; sub-second shell |

Electron's fixed Chromium + Node floor is fundamentally incompatible with a < 500 MB, < 2 s product. Tauri lets the *only* heavy thing be our own engines — which we control and budget. The trade-off (WebView engine differs per OS) is managed by targeting a baseline web feature set and testing on all three WebViews in CI (see §8).

## 2. Cargo workspace layout

One workspace, many crates — mirroring the module map in [02](./02-module-design.md). Crates expose narrow public APIs; internals are private.

```
photon/
├─ crates/
│  ├─ photon-core/        # bus, types, supervision, cancellation
│  ├─ photon-workspace/   # VFS, file tree, sessions, watcher
│  ├─ photon-index/       # incremental index + SQLite store
│  ├─ photon-php/         # tree-sitter parse, resolver, type engine
│  ├─ photon-laravel/     # Laravel intelligence (built on -php + SDK)
│  ├─ photon-nav/         # navigation + Search Everywhere providers
│  ├─ photon-refactor/    # plan/apply change sets
│  ├─ photon-editor/      # rope model, edits, decorations source
│  ├─ photon-db/          # SqlDriver trait + per-engine driver crates
│  │   ├─ driver-mysql/  driver-postgres/  driver-sqlite/  driver-mssql/
│  ├─ photon-git/         # gix-based VCS
│  ├─ photon-terminal/    # portable-pty sessions
│  ├─ photon-debug/       # DBGp (Xdebug) + DAP adapters
│  ├─ photon-ai/          # provider trait + orchestration + context engine
│  ├─ photon-plugins/     # plugin host (wasmtime + restricted node bridge)
│  ├─ photon-settings/    # layered config, keymaps, themes
│  ├─ photon-ipc/         # serde contracts shared with the TS bus client
│  └─ photon-app/         # Tauri bin: wires services, owns the runtime
├─ ui/                    # React/TS app (see doc 08)
├─ plugins-sdk/           # published SDK crates + TS types (see doc 07)
└─ xtask/                 # build/bench/codegen tasks
```

`photon-ipc` is the contract crate: request/event enums live here and a build step **codegens the TypeScript types** from the Rust definitions so the UI and core can never drift.

## 3. Async runtime model

- **One multi-threaded Tokio runtime** owns IO, the dispatcher, and service tasks.
- **One rayon pool** for CPU-bound fan-out (parsing/analysis/index shard builds), run at **lowered OS thread priority/QoS** so background indexing never starves the foreground.
- **Bridging:** CPU work is dispatched to rayon via `spawn_blocking`-style handoff or a dedicated channel; results return as bus events. The dispatcher task itself never blocks.

```rust
// Illustrative service shape
#[async_trait]
trait Service {
    async fn handle(&self, req: Request, cx: Ctx) -> ResponseStream;
}

// Ctx carries: cancellation token, revision, originating view, budget.
```

### Cancellation & backpressure
Every request carries a `CancellationToken`. The UI cancels superseded work (you scrolled, you typed, you navigated away) and the core drops it immediately — this is the mechanism behind perceived instantaneity under load. Channels are bounded; producers respect consumer backpressure so a flood of file-change events can't blow memory.

## 4. The IPC boundary

- Transport: Tauri's IPC for control + a **shared-memory / binary channel** for high-volume payloads (semantic tokens, large search batches) to avoid JSON overhead on hot paths.
- Encoding: `serde` with a compact binary format (bincode/MessagePack) on hot paths; JSON only for low-frequency control messages where debuggability matters.
- **Streaming:** responses are streams of frames tagged with a `query_id` and `done` flag, so the UI renders progressively (first search hits, first diagnostics) instead of waiting for completion.
- **Zero heavy logic crosses the boundary downward:** the UI sends *intents* and *edits*; the core sends *view-models* and *events*.

## 5. Persistence layer

- **SQLite** is the system of record for the index and project intelligence (schema in [03](./03-database-schema.md)). Opened in WAL mode, with memory-mapped IO, prepared-statement cache, and a bounded page cache.
- **Content store:** file contents/hashes kept as needed for diffing and incremental indexing; large blobs are not held resident.
- **Change log (WAL of our own):** an append-only log of index deltas for crash recovery — a hard kill replays the log rather than rebuilding (see [01](./01-system-architecture.md) §8 and [04](./04-indexing-engine.md)).
- **Settings/keymaps/themes:** plain files; a fast-load cache for the startup-critical subset.

## 6. Memory tactics (the < 500 MB contract)

1. **Index on disk, not in RAM.** Symbols/refs live in SQLite; a bounded LRU caches hot rows. Resident memory tracks *open work*, not project size.
2. **Drop trees for closed files.** Tree-sitter trees and semantic models exist only for open/visible documents; closing a file frees them.
3. **Incremental tree reuse.** Edits reuse the prior tree; no full reparse, no transient large allocations.
4. **Arena/interner for symbols.** FQNs and identifiers are interned (`u32` ids) so the symbol graph is compact and dedup'd.
5. **Streaming results, never full materialization.** Search and DB results stream and window.
6. **Per-subsystem budgets + watchdog.** Indexer, AI context, DB grids each have caps; a memory watchdog evicts caches under pressure and surfaces a status indicator rather than OOMing.
7. **Lazy services.** DB tools, debugger, AI providers spin up on first use, not at launch.

## 7. Process & sandbox supervision

- Core services run in-process under a supervisor that restarts a panicked service and reports health to the status bar.
- **Plugins and external LSP servers run as child processes** with CPU/memory rlimits and a capability broker; killing one never affects the editor ([07](./07-plugin-sdk.md)).
- Crash reporting (opt-in) captures a minimal, scrubbed dump for diagnosis.

## 8. Cross-platform layer (L0)

Platform differences are isolated in `photon-workspace` (watcher) and small platform shims:

- **File watching:** FSEvents (macOS), ReadDirectoryChangesW (Windows), inotify/fanotify (Linux), behind one `Watcher` trait with debouncing and rename-coalescing.
- **Keychain:** Security framework / Credential Manager / libsecret behind one `SecretStore`.
- **WebView quirks:** a thin compat shim + CI matrix that runs the UI test suite against WKWebView, WebView2, and WebKitGTK so we catch engine divergence early.
- **Menus / dialogs / tray:** native via Tauri APIs.

## 9. Build, packaging, updates

- `xtask` drives builds, benches, and IPC type codegen.
- Signed, notarized bundles per OS; delta auto-updates via Tauri's updater with staged rollouts.
- A **performance gate in CI**: startup, idle memory, and the large-project benchmark must pass thresholds or the build is blocked. Performance is treated as a test, not a hope.

## 10. Observability

- Structured tracing (`tracing` crate) with spans per request, sampled in production (opt-in).
- A built-in "Photon Doctor" panel surfaces index health, memory budgets, service status, and slow-request traces — both for users and for support.

→ Next: [03 — Database Schema](./03-database-schema.md)
