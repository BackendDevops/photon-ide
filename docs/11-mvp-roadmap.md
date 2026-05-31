# 11 — MVP Roadmap

The MVP's job is to prove the thesis with the smallest credible product: **a fast, native PHP/Laravel editor that an early adopter would use daily instead of VS Code**, hitting the performance targets and delivering enough Laravel magic to be obviously special. We do **not** try to match PhpStorm feature-for-feature at MVP. We win on speed + a few killer Laravel features + polish.

## Strategy: depth over breadth

Cut everything that isn't on the critical path to "a Laravel dev wants this." Defer database tools, full debugging, the marketplace, and most refactorings. Keep: blazing editor, real PHP intelligence, the top Laravel features, navigation, and Search Everywhere.

## Phasing (≈ 6–9 months to a usable MVP, indicative)

### Phase 0 — Foundations (weeks 1–6)
- Tauri shell + React/Tailwind UI skeleton ([08](./08-ui-architecture.md), [09](./09-tauri-rust-backend.md)).
- `core` bus, cancellation, service supervisor; `photon-ipc` contracts + TS codegen.
- `workspace` VFS + native file watching; session persistence.
- Monaco-backed editor surface with Rust-owned rope model; open/edit/save; tree-sitter syntax highlighting.
- **Exit:** open a project, edit files, < 2 s cold start on a medium project, < 500 MB idle. Performance CI gate live from day one.

### Phase 1 — PHP intelligence core (weeks 5–14, overlaps)
- tree-sitter PHP parse → AST/scopes; name resolution (namespaces, `use`, composer autoload).
- `indexer` with incremental per-file deltas + SQLite store ([03](./03-database-schema.md), [04](./04-indexing-engine.md)); persistent warm-start.
- Type engine v1: native types + core PHPDoc (`@param/@return/@var/@property`), traits, interfaces, enums, attributes (parse + store).
- Completion, hover/signatures, semantic highlighting, go-to-definition, find usages.
- **Exit:** go-to-def < 50 ms p95 warm; find-usages correct on a real OSS Laravel repo; index survives restart and reconciles deltas only.

### Phase 2 — Laravel flagship subset (weeks 12–22)
The Laravel features that produce the "whoa" moment, prioritized by impact/effort:
1. **Routes** — discovery, navigation (`route('x')` ↔ controller), name completion, Search Everywhere category.
2. **Eloquent basics** — model detection, relationship inference + navigation, `@property`/casts-based column completion, scope navigation, builder return typing for `first/get/find`.
3. **Blade** — component navigation (`<x-...>` ↔ class/view), view-name navigation/completion, directive completion.
4. **Config & translations** — key navigation + completion for `config()` and `__()`.
- **Exit:** these four feel as good as Laravel Idea on a real project; all incremental (edit a model → relations refresh in ms).

### Phase 3 — Navigation, search & polish (weeks 18–28)
- **Search Everywhere** (double-shift) across files, symbols, routes, models, actions, settings — streamed, < 100 ms first results.
- Recent files/locations, structure view, breadcrumbs, sticky lines, minimap, multi-cursor, column select (Monaco features wired + tuned).
- Integrated terminal (PTY) + artisan/composer/npm task actions.
- Basic git: status, stage/commit, push/pull, inline diff/gutter, branch switch.
- Theme system (dark default), PhpStorm + VS Code keymap presets, settings UI.
- **Exit:** a developer can do a full Laravel feature end-to-end without leaving Photon.

### Phase 4 — AI v1 & hardening (weeks 24–34)
- AI provider layer (Claude/OpenAI/Gemini/local, BYO-key); chat panel with project context; inline completion; "explain"/"apply diff".
- Crash safety, telemetry/Doctor panel, auto-update, signed/notarized builds for macOS/Windows/Linux.
- Cross-WebView CI matrix green; performance gates green on the 1M-file benchmark (editable + warm-start, even if deep index back-fills).
- **Exit / MVP definition of done:** see below.

## MVP definition of done

A Laravel developer can: open a real project in < 2 s, edit at 60 fps with correct completion/navigation/typing, navigate routes/models/Blade/config like Laravel Idea, search anything instantly, run artisan/git from inside, and use AI chat/completion grounded in their project — all under 500 MB idle, on macOS/Windows/Linux, with auto-updates. **Not** required for MVP: DB tools, debugger, full refactoring suite, marketplace, advanced git (interactive rebase), Vue/React deep intelligence.

## Explicit MVP cut list (deferred to Production — [12](./12-production-roadmap.md))
- Database tools (client, schema explorer, ER diagrams).
- Debugger (Xdebug/DAP).
- Full refactoring engine beyond rename (extract/move/change-signature/etc.).
- Plugin SDK + marketplace (internal extension points exist; public SDK later).
- Agent mode (chat + completion first).
- Deep JS/TS/Vue/React intelligence (basic highlighting/LSP-passthrough at MVP).
- Runtime Laravel reflection (static-only at MVP).

## Team shape for MVP
~6–10 engineers: 2 Rust/core+index, 2 PHP/Laravel intelligence, 2 UI/editor, 1 AI, 1 platform/build, plus a designer and a PM. Small, senior, opinionated.

## Risks gating the MVP
The two existential ones: (1) PHP/Laravel intelligence quality reaching "trustworthy," and (2) holding performance targets as features land. Both are tracked from day one via golden-file correctness suites and the CI performance gate. See [14](./14-risk-analysis.md).

→ Next: [12 — Production Roadmap](./12-production-roadmap.md)
