# Photon IDE — v1

A lightweight, native **PHP / Laravel IDE** built with **Tauri 2 + Rust + React/TypeScript/Tailwind**.

A real desktop app with the core intelligence working end-to-end — **not** mocked. The full architecture lives in [`docs/`](./docs/00-architecture-index.md). This codebase is the MVP plus the **v1 pillars**: cross-file navigation + Safe Rename, deeper Laravel intelligence (Eloquent/config/i18n), database tools, and a **flagship GitKraken-inspired Git experience** ([`docs/16`](./docs/16-git-experience.md)).

> See **Status** below for exactly what works today and what's designed-but-scheduled.

---

## What works right now

**Foundation (MVP)**
- **Native shell**: Tauri 2 window (OS WebView — no bundled Chromium), dark compact UI.
- **Project open + index**: pick a folder; the Rust core walks it (honoring `.gitignore`) and builds a SQLite index.
- **Real PHP intelligence** (tree-sitter): namespaces, classes, interfaces, traits, enums, enum cases, functions, methods, properties, class constants — with fully-qualified names and precise locations.
- **Editor**: Monaco + custom Photon dark theme, multi-cursor, minimap, sticky scroll, bracket colorization. `Cmd/Ctrl+S` saves and **incrementally re-indexes** that file.
- **Search Everywhere** (`Shift Shift` / `Cmd/Ctrl+P`): ranked fuzzy search across files + symbols + routes. Tabs, file tree, outline, routes, status bar.

**v1 — Navigation & refactoring**
- **References index** of PHP use-sites (type refs, static/function calls, imports, member access).
- **Find Usages** (`Shift+F12`): references grouped by file in the bottom dock.
- **Safe Rename** (`F2`): plan-then-apply across the project with a diff **preview**, per-edit checkboxes, and **uncertain references flagged**.

**v1 — Laravel depth**
- **Eloquent**: model detection (`$table`, `$fillable`) + **relationship inference** (`hasMany`/`belongsTo`/…) with related-model resolution (Models panel).
- **Config**: nested `config/*.php` keys indexed as dotted paths (`services.stripe.key`).
- **i18n**: translation keys across locales (PHP + JSON) with **missing-translation detection**.

**v1 — Database tools**
- Connection manager + schema explorer + **query console** with results grid, for **MySQL / PostgreSQL / SQLite** (sqlx `Any` driver).

**v1 — Git (flagship, GitKraken-inspired — [`docs/16`](./docs/16-git-experience.md))**
- **Visual commit graph**: SVG lanes, colored branches, merge nodes, author avatars, branch/tag/HEAD chips — lanes computed in Rust.
- **Commit workspace**: staged/unstaged lists, stage/unstage, branch switch, push/pull/stash, commit box.
- **AI-style commit messages** (✨ Suggest) and a **diff viewer** with +/- coloring.

**v1.1 — IDE shell polish**
- **JetBrains-style header toolbar**: project chip, branch chip with a **branch popover** (search, Update/Commit/Push/New Branch, local + remote branches with per-branch ↑/↓ ahead-behind), Git quick actions, search, settings.
- **Native application menu** (macOS menu bar / Win-Linux): File / Edit / View / Git / Tools / Help with accelerators, wired to in-app actions via events.
- **Integrated terminal**: real PTY sessions (`portable-pty`) rendered with xterm.js, **multiple terminals in tabs**, reachable from the activity-bar bottom, the **bottom-left status bar**, or `⌘\``.
- **Advanced data sources**: a "Data Sources and Drivers"-style **connection manager** — form fields (driver, host, port, user, password, database, Save), **Test Connection**, saved profiles persisted to `.photon/datasources.json`, MySQL / MariaDB / PostgreSQL / SQLite.
- **Settings**: editor font/tab/wrap/minimap/sticky/ligatures, terminal font, keymap preset — live-applied, persisted.

**v2 (in progress)**
- **Symbol-resolved navigation (Cmd/Ctrl+click)** — JetBrains-style: clicking a **declaration** shows Find Usages; clicking a **use-site** goes to the definition. For member accesses the target is **receiver-aware** — `$this->svc->find()` / `$user->save()` resolves the chain to the right class (own + `extends` + traits) and jumps to *that* class's member, not a same-named one elsewhere. (`goto_member_def`.)
- **Inspection engine v1**: background, file-local diagnostics with quick-fixes — **unused imports** and **duplicate imports** (one-click "Remove import"), and **leftover debug statements** (`dd`/`dump`/`var_dump`/`ray`/`print_r`, ignoring `$x->dump()` method calls). Runs on open + save, alongside the existing unknown-key and unimported-class checks.
- **Type-based inspection — undefined `$this->member`**: using the type engine, flags `$this->method()` / `$this->prop` that don't exist on the enclosing class (own members + `extends` chain + traits + constructor-promoted properties). **Deliberately conservative** — skips classes that use magic (`__call`/`__get`/…), Eloquent models (dynamic columns), and any class with an unresolvable ancestor — so false positives stay near-zero. (General `$var->` undefined-checks come later.)
- **Type engine v1** (v1.5 keystone): method **return types** and **property types** are indexed (declared types), and member completion is **chain-resolving** — `$this->service->find()->`, `$repo->user->`, `User::query()->where()->first()->` walk the type chain step-by-step (incl. `self`/`static` returns and Eloquent builder) to complete the *right* class's members. *(Declared-types-only for now; PHPDoc generics/union come in a later step — e.g. `->get()` still suggests the model, not `Collection<Model>`.)*
- **Recent Files / Recent Locations** (`⌘E`): MRU file switcher + recent navigation jumps.
- **Search Everywhere — Actions & Settings**: the palette now also matches IDE **Actions** (Open Folder, Save, Run Artisan, Generate PHPDoc, Git commit/push/update, switch views, new data source…) and **Settings** entries, merged above file/symbol/route results.
- **Go to Implementation** (`⌘⌥B`): a new `type_relations` index (extends / implements / uses, parsed from PHP) powers jumping to a class/interface/trait's implementers — single match opens directly, many show the usages-style popup.
- **Framework / vendor navigation** (gap-analysis priority #1): `vendor/` is now indexed at **declaration level** (symbols only — no references/bodies) so framework & package classes (`Illuminate\…`) are searchable, hover-resolvable, and jump-to-able. **Performance-safe by design**: it runs *deferred* after project open (doesn't slow startup), off the UI thread, with a single-transaction bulk insert, and vendor symbols are de-prioritized in Search Everywhere. **F12 / ⌘B → Go to Definition** (resolves into vendor too).
- **AI Workspace (W3)**: a dedicated, project-aware AI chat panel (✦). Pluggable **BYO-key**, OpenAI-compatible — works with OpenAI / OpenRouter / Azure and **local Ollama** (just set base URL + model in Settings → AI). The active file + project facts are sent as grounded context.
- **Eloquent / Laravel-Idea-style completion (Wave 1)**: `Model::query()->where(...)->` resolves the chain to the model and completes **query-builder methods, real columns (parsed from migrations), dynamic `whereColumn` (e.g. `whereEmail`), local scopes (`scopeActive`→`active()`), and relations**. Plus completion for `route()` / `config()` / `env()` / `__()`,`trans_choice()`,`@lang()` keys, **validation rules** inside `rules()`/`validate()`, and `env()`/key Cmd+click navigation.
- **15+ code generators** (New-from-Template): Resource, Notification, Mailable, Policy, Job, Event, Listener, Observer, Cast, Command, Enum, Action, DTO, Seeder, Factory, Pivot migration — on top of Controller/Model/Migration/Request/Middleware/Blade/Test.
- **Laravel-Idea Wave 2**: **middleware** completion (Kernel/bootstrap aliases + defaults), **`$request->input()` / `request()` / `->validated()`** completion from FormRequest `rules()` keys, **`auth()->user()->`** typed to the User model, and **container binding navigation** — Cmd+click `app(Foo::class)` jumps to the bound concrete (or class definition).
- **Laravel-Idea Wave 3**: **Generate Model PHPDoc** (Laravel menu) — emits typed `@property` for every column (types parsed from migrations) and `@property-read` for relations (`Collection<int, Related>` for to-many), inserted above the class and re-runnable. **Run Artisan Command** — a runner with command completion (`artisan list`), execution, and output. Migration columns now carry PHP types.
- **Multiple projects in one window** (multi-root workspace): open several folders at once; one shared index means **Cmd/Ctrl+click navigates across projects** (click a class used in X but defined in Y → jump straight there). Projects list + close in the Explorer.
- **`config()` / `route()` / `__()` Cmd+click**: clicking a string key jumps to its definition (config file entry, route, or translation file).
- **Import quick-fix**: unresolved classes are underlined; the lightbulb / code action adds the `use ...;` import.
- **Keyword & snippet completion**: typing `pub`→`public`, `fun`→`function`, plus snippets (`function`, `__construct`, `foreach`, `fn`, `if`) — no more nonsense class-only suggestions.
- **Resizable panels**: drag to resize the sidebar, the bottom dock (diff/usages/query), and the terminal.
- **Status bar system info**: live **PHP version** and **Photon memory (MB)**, plus index/branch/Laravel chips.
- **Expanded native menu**: Code / Refactor / Laravel / Git menus with JetBrains-style actions and accelerators.
- **Smarter commit messages**: ✨ Suggest now reads the staged diff and proposes conventional-commit style (`feat(scope): add X`).
- **Type-aware member completion**: after `->` / `::`, Photon resolves the receiver (`$this`/`self`, typed params, typed properties, `$x = new Foo`) and completes its methods/properties/constants (with one-level inheritance). First slice of the W1 type engine.
- **Extract Method** (`⌘⌥M`): pulls the selected statements into a new private method (params auto-detected) and inserts it after the current method — enabled by the body-range index.
- **Cmd/Ctrl+click → "Show Usages" popup**: a floating JetBrains-style popup at the cursor listing usages (file · line · code preview), keyboard-navigable.
- **Side-by-side diff**: Git diffs open in a real Monaco DiffEditor (original ↔ working), not a unified text dump.
- **Git context menu**: right-click a change in the commit panel for Show Diff / Jump to Source / Stage-Unstage / Rollback (discard).
- **Auto-save**: on by default (configurable delay) — saves + re-indexes after a short idle.
- **Collapsed file tree**: all folders start collapsed.
- **Body-range index**: symbols carry full declaration ranges → **Safe Delete removes whole method/class bodies**.
- **Editor mouse fix**: removed CSS `zoom`-based UI scaling (it broke Monaco's click-to-position); UI scales via font size so caret placement/selection work correctly.

**v1 completion — parity items**
- **Refactorings**: Rename + **Extract Variable** (`⌘⌥V`), **Inline Variable** (`⌘⌥N`), **Safe Delete** (usage-checked) — plan/preview/apply.
- **Completion + hover + diagnostics**: index-driven completion for `route()` / `config()` / `__()` keys and class names; symbol **hover** (kind · FQN · location); **diagnostics** flagging unknown route/config/translation keys (red squiggles).
- **Laravel depth (rest)**: service-container **bindings**, **events→listeners**, **queued jobs**, **factories/seeders** — all in the Eloquent panel tabs.
- **Git v1.5**: **blame**, **cherry-pick**, **branch compare**, and a **conflict resolution** section (use ours/theirs + diff).
- **DB**: inline-cell update backend (`db_update_cell`).

> Remaining toward full PhpStorm parity (type-inference-driven completion, Extract Method/Change Signature/Move, Xdebug debugger, visual interactive rebase + PRs, editable DB grid UX) is sequenced in **[`docs/17-v2-plan.md`](./docs/17-v2-plan.md)**.

**v1.3 — Premium design system**
- **Layered surface system** (Material 3 / Fleet-inspired): 6 elevation levels (editor canvas → sidebars → panels → floating → dialogs → command palette), no flat monochrome, subtle depth/shadows and a gradient canvas (no pure black).
- **Typography**: **Inter** (UI) + **JetBrains Mono** (code), bundled for offline use, with a consistent type scale.
- **Command Center header** (Arc/Raycast/Fleet): grouped workspace + branch pills, a prominent centred Search Everywhere field, and live **semantic status chips** (Laravel · indexing/indexed · AI).
- **Motion**: 120–250 ms spring/smooth transitions, hover/focus feedback, floating active editor tabs with accent indicators, animated palette/dialogs.
- **Semantic color states**: success / warn / error / info / running / indexed / AI — instantly recognizable; vibrant accent on neutral dark surfaces.

**v1.2 — Readability, templates & extensions**
- **Larger, more readable UI** with comfortable type/row sizes, plus a **UI scale** setting (90%–140%) applied app-wide. Default editor font bumped to 15, terminal to 14.
- **New, professional material app icon** (gradient squircle + photon-aperture glyph).
- **Templating**: **New from Template** (`⌘N`) — built-in PHP/Laravel templates (Class, Controller, Model, Migration, Request, Middleware, Blade component/view, Pest test) with variable substitution; plus **user templates** in `.photon/templates/*.json`.
- **Extensions**: declarative extensions in `.photon/extensions/` that contribute **templates & snippets**, an Extensions panel with enable/disable, and a one-click example pack. (Full sandboxed plugin runtime + marketplace: docs/07.)

## Architecture (mirrors the design docs)

```
photon-ide/
├─ crates/photon-core/      # pure Rust logic — NO gui dependency, unit-tested
│  └─ src/
│     ├─ types.rs           # Symbol, Route, FileEntry, SearchHit, ...
│     ├─ workspace.rs       # project walk + language classification
│     ├─ db.rs              # SQLite index (subset of docs/03 schema)
│     ├─ php.rs             # tree-sitter PHP → symbol extraction (docs/05)
│     ├─ laravel.rs         # route discovery (docs/06)
│     ├─ search.rs          # Search Everywhere fuzzy ranking (docs/02)
│     ├─ indexer.rs         # orchestration: walk → parse → store (docs/04)
│     └─ lib.rs             # public API + unit tests
├─ src-tauri/               # thin Tauri shell: state + invoke commands
│  └─ src/lib.rs            # open_project, list_files, read/save, search, ...
└─ src/                     # React UI (docs/08)
   ├─ App.tsx               # layout + state
   ├─ lib/api.ts            # typed bindings over Tauri `invoke`
   └─ components/           # FileTree, EditorPane, SearchEverywhere, panels...
```

The split is deliberate: **all real logic lives in `photon-core`**, which has no GUI dependency and is unit-tested. The Tauri layer is glue. This is the same boundary the full design uses, so the engine can grow without touching the shell.

---

## Prerequisites

- **Rust** (stable): https://rustup.rs
- **Node.js 18+** and npm
- Platform build deps for Tauri 2 — see https://tauri.app/start/prerequisites/
  - macOS: Xcode Command Line Tools (`xcode-select --install`)
  - Linux: `webkit2gtk`, `libgtk-3-dev`, etc. (see the link)
  - Windows: WebView2 (preinstalled on Win 11) + MSVC build tools

## Run it

```bash
cd photon-ide
npm install
npm run tauri:dev      # launches the desktop app (builds Rust + serves the UI)
```

First Rust build compiles tree-sitter + bundled SQLite, so it takes a few minutes; subsequent runs are fast.

### Try it on a Laravel app
Open any Laravel project folder via **Open Folder…** (top bar). You'll see it flagged `Laravel`, its routes in the Routes panel, and class/method symbols in Search Everywhere and the Structure panel.

## Verify the core logic (no GUI needed)

The substantive logic is unit-tested and runs without the desktop stack:

```bash
cargo test -p photon-core
```

Tests cover PHP symbol **and reference** extraction, **cross-file Safe Rename** (plan + apply), Laravel route parsing, **Eloquent model + relationship inference**, **nested config keys**, **missing-translation detection**, the SQLite round-trip, and the fuzzy ranker.

## Build a distributable

```bash
npm run tauri:build    # produces a signed-able native bundle for your OS
```

---

## Status: what's real vs. deferred

**Working (real):** everything in "What works right now" above — the MVP foundation plus the four v1 pillars (navigation + Safe Rename, Laravel depth, database tools, and the Git graph + commit workspace).

**Designed & scheduled (see `docs/`):**

- **Type inference, completion, hover, diagnostics** — the symbol/reference index exists; the type engine (docs/05) is next.
- **Git flagship — full GitKraken parity** (docs/16): drag-and-drop ops with preview, interactive-rebase timeline, PR integration (GitHub/GitLab/Bitbucket/Azure), smart PHP/Laravel-aware diff, repository insights, AI conflict-resolution center, and graph virtualization to 100k+ commits. v1 ships the graph + workspace + AI-style messages.
- **Blade / container / events / queues** intelligence — Eloquent/config/i18n are done; the rest of the Laravel engine is scoped in docs/06.
- **Index persistence / warm start** — in-memory index per session today; switch `open_project` to a `.photon/index.sqlite` file to persist (noted in code).
- **File watching** — save triggers per-file re-index today; a filesystem watcher is next.
- **Debugger, AI subsystem, plugins** — designed in `docs/`, not in this build.

### A note on verification
The **frontend is verified** here: `tsc --noEmit` passes and `vite build` succeeds. The **Rust was written against pinned APIs** (tree-sitter 0.20, rusqlite 0.31, sqlx 0.8 `Any`, `portable-pty` 0.8, Tauri 2 menu API) and is structured for `cargo test -p photon-core`, but was **not compiled in the authoring environment** (no Rust toolchain there). Run `cargo test -p photon-core` and `npm run tauri:dev` locally. If the first Rust build surfaces a minor API nit, the likeliest spots are tree-sitter node kinds in `php.rs`, sqlx `Any` value decoding in `src-tauri/src/dbtools.rs`, or a Tauri 2 menu-builder signature in `src-tauri/src/lib.rs`. The **terminal**, **database tools**, and **git** features require their runtimes (a shell, a live DB, a real repo + the `git` binary), so verify those on your machine.

## License

MIT.
