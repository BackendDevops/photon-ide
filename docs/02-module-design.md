# 02 — Domain-Driven Module Design

Photon is decomposed into **16 modules**, each a bounded context with an explicit public contract and a private implementation. Modules communicate only through their published interfaces and the command bus — never by reaching into each other's internals. This is what makes each module *independently replaceable* (a stated product requirement).

```
core/        editor/      workspace/   indexer/
navigation/  refactoring/ database/    terminal/
git/         debugger/    plugins/     laravel/
php/         ai/          settings/    ui/
```

## Bounded-context map

```
                         ┌──────────┐
                         │  core    │  (kernel: bus, lifecycle, types)
                         └────┬─────┘
        ┌──────────┬─────────┼──────────┬───────────┬──────────┐
        ▼          ▼         ▼           ▼           ▼          ▼
   workspace ─► indexer ─► php ─────► laravel    navigation  refactoring
        │          │        ▲           ▲            ▲            ▲
        │          └────────┴───────────┘            └────────────┘
        ▼                       (intelligence)
     editor ◄──── ui ◄──── settings        database   git   terminal   debugger
        ▲                                      ▲        ▲       ▲          ▲
        └──────────────── plugins ─────────────┴────────┴───────┴──────────┘
                            (extends everything)        ai (orthogonal)
```

Each module below lists its **responsibility**, **public contract**, **key dependencies**, and **replaceability note**.

---

## 1. `core` — the kernel

**Responsibility.** Process lifecycle, the typed command bus, cancellation tokens, the shared type vocabulary (`DocId`, `Position`, `Range`, `Revision`, `SymbolId`), the service registry/supervisor, and the event broker.

**Public contract.** `Bus::request(Request) -> ResponseStream`, `Bus::subscribe(EventKind)`, `ServiceRegistry::register/get`, supervision hooks.

**Depends on.** Nothing above L0.

**Replaceable?** No — this is the spine. But it is deliberately tiny: it knows about messages and lifecycles, not about PHP, editors, or databases.

---

## 2. `workspace` — project model

**Responsibility.** What "a project" is: roots, file tree, virtual filesystem abstraction, file metadata (mtime, size, language), open-file set, sessions, multi-root workspaces, and `.gitignore`/`vendor` scoping rules.

**Public contract.** `Workspace::roots()`, `files(filter)`, `watch() -> FileChangeStream`, `read(path)/write(path)`, `session().restore()/save()`.

**Depends on.** `core`, L0 platform (FS + watcher).

**Replaceable?** Yes — a remote/SSH or container-backed VFS can be swapped in without touching consumers (this is how remote dev later plugs in; see [13](./13-scaling-strategy.md)).

---

## 3. `indexer` — incremental index

**Responsibility.** Turning files into a queryable symbol/intelligence database; incremental updates on change; persistence; backpressure. The full design is in [04 — Indexing Engine](./04-indexing-engine.md).

**Public contract.** `Index::symbols(query)`, `references(symbol)`, `definitions(name)`, `apply_delta(FileDelta)`, `progress() -> ProgressStream`.

**Depends on.** `workspace`, `php`, `laravel`, persistence (L1).

**Replaceable?** The *storage* (SQLite) and the *extractors* (php/laravel) are pluggable; the orchestration is stable.

---

## 4. `php` — PHP analysis engine

**Responsibility.** Parse (tree-sitter) → resolve → infer. Namespaces, types, generics (PHPDoc-based), traits, interfaces, abstract classes, magic methods/properties, attributes, enums, anonymous classes, dynamic return types. Full design in [05](./05-php-analysis-engine.md).

**Public contract.** `Php::analyze(doc) -> SemanticModel`, `resolve(name, scope) -> Vec<FqName>`, `type_of(expr) -> TypeRef`, `members_of(type) -> Vec<Member>`.

**Depends on.** `core`, tree-sitter.

**Replaceable?** Yes — and notably, an external LSP server (Intelephense/Phpactor) can be slotted behind the same `Php` contract as a fallback or supplement.

---

## 5. `laravel` — Laravel intelligence engine

**Responsibility.** The flagship. Routes, Eloquent relationships/scopes/factories/seeders, the service container & bindings, Blade components/slots/props/directives, config, localization, events/listeners, queues/jobs. Full design in [06](./06-laravel-intelligence.md).

**Public contract.** `Laravel::routes()`, `model(name) -> ModelInfo`, `resolve_binding(abstract)`, `blade_component(tag)`, `config_key(path)`, `translation(key)`.

**Depends on.** `php`, `indexer`, `workspace`.

**Replaceable?** Yes — built entirely on the public plugin SDK as proof the SDK is sufficient. Could be versioned/shipped independently of the core.

---

## 6. `editor` — text editing surface

**Responsibility.** The authoritative document model (rope), edit application, multi-cursor, column selection, folding model, the data behind minimap/sticky-lines/breadcrumbs, semantic/symbol highlighting, inline & parameter hints, code lens. The *rendering* is in `ui`; `editor` owns the *model and operations*.

**Public contract.** `Editor::open(path) -> DocId`, `apply(edits)`, `selections()`, `folding_ranges()`, `decorations()`, `semantic_tokens()`.

**Depends on.** `core`, `php`/`laravel` (for tokens/hints, via bus).

**Editor technology decision.** We start with **Monaco** for the view layer (battle-tested, gives us multi-cursor, folding, minimap, column select for free) but with a **custom model bridge**: the authoritative rope and edit pipeline live in Rust, and Monaco is driven as a view. This avoids Monaco's weakness (everything-in-JS for huge files) while keeping its mature UX. A later milestone evaluates replacing Monaco's renderer with a custom WebGL/Canvas surface if profiling demands it; the `editor` contract is designed so the view layer is swappable without touching anything else. See [08](./08-ui-architecture.md) §editor.

**Replaceable?** The view layer (Monaco → custom) yes; the model is core.

---

## 7. `navigation` — code navigation

**Responsibility.** Go to Definition / Declaration / Implementation / Type, Find Usages, Show Usages (inline popup), Recent Files, Recent Locations, symbol navigation, and **Search Everywhere** (double-shift). Latency is the headline feature.

**Public contract.** `Nav::definition(loc)`, `implementations(loc)`, `usages(symbol) -> Stream`, `search_everywhere(query, scopes) -> Stream`.

**Search Everywhere design.** A single ranked, streamed query across providers: Files, Classes, Methods, Symbols, **Actions** (commands), **Settings**, **Database objects**, **Routes**, **Models**. Each provider is a trait `SearchProvider { fn search(&self, q, budget) -> Stream<Hit> }`; the navigation module merges and ranks (fuzzy score × recency × kind-weight) and streams the top-K as they arrive so first results paint < 100 ms. Providers are registered by core modules and plugins alike.

**Depends on.** `indexer`, `php`, `laravel`, `database`, `settings`.

**Replaceable?** Providers are plugins; the ranking/merge core is stable.

---

## 8. `refactoring` — safe transformations

**Responsibility.** Rename, Extract Method, Extract Variable, Move Class, Move Namespace, Change Signature, Safe Delete, Inline Variable — each updating *all* references atomically with preview.

**Public contract.** `Refactor::plan(op) -> ChangeSet` (preview), `apply(ChangeSet) -> Result`. A `ChangeSet` is a set of cross-file `TextEdit`s plus file moves/creates/deletes, validated against the live index.

**Design principle.** Refactorings are **plan-then-apply**: compute a full change set against semantic facts (not text search), show a diff/conflict preview, then apply transactionally with undo as one unit. Rename of a method consults the `php`/`laravel` engines for *every* reference including dynamic ones flagged as "uncertain" for user confirmation — correctness over silent breakage.

**Depends on.** `php`, `laravel`, `indexer`, `editor`, `workspace`.

**Replaceable?** Each refactoring is a registered `Refactoring` implementation; new ones (incl. plugin-provided) drop in.

---

## 9. `database` — database tools

**Responsibility.** A built-in DB client: connection manager (MySQL, PostgreSQL, MariaDB, SQLite, SQL Server), schema explorer, query runner with result grid, EXPLAIN plan visualization, table editor (CRUD on rows), ER diagrams.

**Public contract.** `Db::connect(profile)`, `schema(conn)`, `run(conn, sql) -> ResultStream`, `explain(conn, sql)`, `er_diagram(conn, scope)`.

**Design notes.** Drivers behind a `SqlDriver` trait (per-engine crates: `sqlx`-backed where possible). Result grids stream + virtualize (paged fetch) so a `SELECT *` on a huge table never materializes fully. Schema is itself indexed so **DB objects appear in Search Everywhere** and Laravel can correlate Eloquent models ↔ tables/columns. Credentials stored in the OS keychain via L0.

**Replaceable?** Drivers are pluggable; a plugin can add a new engine.

---

## 10. `git` — version control

**Responsibility.** Commit, push, pull, fetch, rebase (incl. interactive), cherry-pick, stash management, branch compare, blame, inline diff/gutter, conflict resolution UI.

**Public contract.** `Git::status()`, `commit(msg, files)`, `push/pull/fetch`, `rebase(onto, interactive_plan)`, `stash_*`, `diff(a,b)`, `blame(file)`.

**Design notes.** Backed by `gix` (gitoxide, pure-Rust) for speed and to avoid shelling out where possible, with a libgit2/CLI fallback for operations gix doesn't yet cover (e.g. complex interactive rebase orchestration is composed from primitives + a guided UI). Status is computed incrementally off the same file watcher the workspace uses.

**Replaceable?** Backend (gix ↔ libgit2 ↔ CLI) swappable behind the contract.

---

## 11. `terminal` — integrated terminal

**Responsibility.** PTY-backed terminals, multiple sessions/tabs/splits, shell integration (cwd tracking, command markers), and task running (artisan, composer, npm). 

**Public contract.** `Terminal::spawn(shell, cwd) -> TermId`, `write(id, bytes)`, `resize`, `output() -> Stream`.

**Design notes.** Real PTY via `portable-pty`; rendering via a virtualized terminal grid in the UI. Laravel/Composer/npm task discovery feeds Search Everywhere actions ("artisan migrate", "composer install").

**Replaceable?** Renderer swappable; PTY layer stable.

---

## 12. `debugger` — debugging

**Responsibility.** PHP debugging via **Xdebug (DBGp protocol)**, plus a generic **DAP (Debug Adapter Protocol)** host for JS/TS/Node front-ends. Breakpoints, step in/out/over, call stack, scopes/variables, watches, conditional & log breakpoints.

**Public contract.** `Debug::start(config) -> SessionId`, `set_breakpoints`, `step(kind)`, `evaluate(expr)`, `stack()/scopes()`.

**Design notes.** A DBGp client (Xdebug) and a DAP client are two adapters behind one `DebugAdapter` trait, so Vue/React/TS debugging reuses the same UI. Launch configs are project files (versionable), with Laravel-aware presets (e.g., "Debug current artisan command", "Debug PHPUnit test").

**Replaceable?** Adapters are pluggable; the debug UI/state is stable.

---

## 13. `plugins` — extensibility host

**Responsibility.** Loading, sandboxing, lifecycle, capability grants, versioned API surfacing, hot reload, and the marketplace client. Full spec in [07](./07-plugin-sdk.md).

**Public contract.** `Plugins::install/enable/disable`, `host(manifest) -> PluginHandle`, capability broker.

**Design notes.** Plugins run **out-of-process** (WASM component model preferred; restricted Node for ecosystem-compat plugins) and may *never* block the UI thread — they communicate over the same async bus, mediated by a capability broker that enforces declared permissions.

**Replaceable?** The runtime backends (WASM/Node) are pluggable.

---

## 14. `ai` — AI subsystem

**Responsibility.** Chat panel, inline completion, agent mode, project-wide context, code explanation, refactoring suggestions, across pluggable providers (Claude, OpenAI, Gemini, local). Full design in [10](./10-ai-subsystem.md).

**Public contract.** `Ai::complete(ctx) -> Stream`, `chat(thread, msg, ctx) -> Stream`, `agent(goal, tools) -> Run`, `context(query) -> ContextBundle`.

**Depends on.** `indexer`, `php`, `laravel` (for context), `editor`, `refactoring` (agent edits go through the same `ChangeSet` plan/apply path as refactorings — safety reuse).

**Replaceable?** Providers behind a trait; orthogonal to everything else.

---

## 15. `settings` — configuration

**Responsibility.** Layered settings (default → user → workspace → folder), keymaps (with PhpStorm and VS Code preset keymaps), themes, per-project intelligence config (e.g. Laravel root override), and the settings store used at startup.

**Public contract.** `Settings::get(key)`, `set(scope, key, val)`, `watch(key)`, `keymap()`, `theme()`.

**Design notes.** Settings are plain files (JSON/TOML) so they're versionable and diffable. The fast-loading subset needed at T0/T1 (theme, layout, keymap) is cached separately for startup speed (see [01](./01-system-architecture.md) §6).

**Replaceable?** Schema-driven; modules and plugins register their settings schemas.

---

## 16. `ui` — presentation

**Responsibility.** The React/TS/Tailwind presentation layer: shell layout, dockable/floating panels, command palette, editor view binding, theming, animations, virtualization. Full design in [08](./08-ui-architecture.md).

**Public contract.** Consumes view-models over the bus; emits intents. Holds no domain logic.

**Replaceable?** It's the most volatile layer by design; because it holds no logic, it can be redesigned freely.

---

## Dependency rules (enforced in CI)

1. `core` depends on nothing above it.
2. Intelligence (`php`, `laravel`) never depends on `editor`/`ui`.
3. `ui` depends on no domain module's internals — only on view-models and the bus.
4. `plugins` may depend on published contracts only; the same contracts third parties get.
5. Cycles are forbidden and checked by a build lint (`cargo-deny`-style + a TS boundary lint).

These rules are what let a team of dozens work in parallel without stepping on each other, and what let any single module be rewritten behind its contract.

→ Next: [03 — Database Schema](./03-database-schema.md)
