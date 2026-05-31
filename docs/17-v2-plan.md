# 17 — v2 Plan ("Better than PhpStorm for Laravel")

v1 made Photon a credible daily driver: fast native shell, real PHP/Laravel intelligence, navigation + refactorings, database tools, a GitKraken-style git start, integrated terminal, templates, extensions, and a premium design system. **v2 closes the remaining gaps to full PhpStorm/JetBrains parity and then pushes past it** on the axes Photon is built to win — type intelligence, a flagship Git experience, an AI workspace, and a real plugin platform.

This document is the v2 roadmap. It supersedes the v2.0 bullets in [`12-production-roadmap.md`](./12-production-roadmap.md) with concrete, sequenced workstreams.

---

## Where v1 landed (honest baseline)

**Done & working:** editor, symbol + reference index, Search Everywhere, cross-file go-to-def, Find Usages, **Rename / Extract Variable / Inline Variable / Safe Delete (single-line)**, Laravel **routes / Eloquent / config / i18n / container bindings / events / queues-jobs / factories-seeders**, **index-driven completion (route/config/__/class) + hover + key diagnostics**, DB connection manager + schema explorer + query console (+ `db_update_cell` backend), Git (graph, workspace, blame, cherry-pick, branch compare, conflict resolve, AI-style messages), terminal, templates, extensions, design system.

**Partial / foundation only:**
- Completion is **index-driven, not type-driven** (no member completion from inferred types).
- Safe Delete only removes single-line members (no body-range index yet).
- DB inline edit has a backend command but no full editable-grid UX.
- Git conflict resolution is ours/theirs + diff (no 3-way visual merge); no visual interactive rebase.

**Not started:** type-inference engine v2, debugger, PR integration, AI agent workspace, plugin runtime, DB visual builder/ER diagrams, remote dev.

---

## v2 workstreams (sequenced)

### W1 — Type Intelligence Engine v2 *(the keystone)*
The single highest-leverage gap. Everything below benefits from it.
- **Body-range index**: store full symbol ranges (enables Safe Delete of methods/classes, Extract Method, structural refactors).
- **Type inference**: native types + PHPDoc generics solver + flow narrowing + array shapes ([`05-php-analysis-engine.md`](./05-php-analysis-engine.md) §4). Confidence-scored.
- **Type-driven features**: member completion after `->`/`::`, accurate hover types, parameter hints, "unresolved member / wrong arg" diagnostics.
- **Dynamic-return-type providers** so Eloquent/builder/`app()` chains type correctly — wires into the Laravel engine already present.
- **Extract Method, Change Signature, Move Class/Namespace** refactorings (need ranges + types).
*Exit:* completion/hover/diagnostics reach PHPStorm-class accuracy on annotated code; full refactoring suite.

### W2 — Git flagship (GitKraken parity) — see [`16-git-experience.md`](./16-git-experience.md)
- **Drag-and-drop graph ops** with visual preview (move/merge/reset/cherry-pick/rebase).
- **Interactive-rebase timeline** (reorder/squash/fixup/drop/edit/split).
- **3-way visual conflict center** with AI-assisted resolution (replaces ours/theirs buttons).
- **PR integration**: GitHub / GitLab / Bitbucket / Azure behind one `PrProvider` trait — create/review/approve/CI status/inline comments.
- **gitoxide backend** + graph **virtualization to 100k+ commits**.
- **Repository insights**: hotspots, contributors, conflict zones, velocity.

### W3 — AI Workspace
- **Dedicated AI panel** with agent status, **context-awareness visualization** (which files/symbols/Laravel facts are in context), task history, multi-step agent runs.
- **Provider layer** (Claude / OpenAI / Gemini / local) — [`10-ai-subsystem.md`](./10-ai-subsystem.md); BYO-key.
- **Safe agent edits** through the existing ChangeSet plan/apply path (reuse refactoring engine).
- Upgrade the heuristic **commit-message generator** to LLM; add PR summaries, conflict explanation, pre-merge risk analysis.

### W4 — Debugger (Xdebug / DAP)
- **DBGp listener** for Xdebug: breakpoints (line/conditional/log), step in/out/over, call stack, scopes/variables, watches, evaluate.
- **DAP host** for JS/TS so Vue/React debugging reuses the same UI.
- Laravel-aware launch presets ("Debug current artisan command", "Debug PHPUnit/Pest test").

### W5 — Database Workspace (DataGrip/TablePlus-class)
- **Editable data grid** wired to `db_update_cell` (PK-aware), insert/delete rows, transactions.
- **Tabbed SQL editor** with history + per-connection consoles.
- **Visual query builder** and **relationship/ER diagrams**.
- EXPLAIN-plan visualization; Redis inspector; **Eloquent↔schema validation** (flag `$model->unknown_column`).

### W6 — Frontend & ecosystem intelligence
- Real **Vue/React/TypeScript** (TS server + tree-sitter), Tailwind class completion, Inertia page↔component nav, Blade↔Vue boundary.
- First-party packs (Livewire, Filament, Inertia, Pest) on the extension API.

### W7 — Plugin platform (the real SDK) — [`07-plugin-sdk.md`](./07-plugin-sdk.md)
- Out-of-process **WASM/Node runtime** with the capability broker (today's extensions are declarative templates/snippets only).
- **Marketplace** with signing, reviews, versioned APIs, hot reload, private/enterprise registries.

### W8 — Platform & scale
- **Remote / SSH / container dev** (swap the workspace VFS — [`13-scaling-strategy.md`](./13-scaling-strategy.md)).
- **Index persistence / warm start** (`.photon/index.sqlite`) + **filesystem watcher** for live incremental indexing (today: in-memory per session + per-file reindex on save).
- OS-keychain for DB/AI secrets (today: `.photon/datasources.json`).
- Enterprise: SSO/SAML, policy controls, audit, fleet management — [`15-monetization.md`](./15-monetization.md).

---

## Sequencing & rationale

```
v2.0  ── W1 Type engine ──┐         (unlocks completion/refactors — top priority)
        W4 Debugger       │ parallel team
v2.1  ── W2 Git flagship  │         (drag-drop, rebase, PRs, conflict center)
        W5 DB workspace   │
v2.2  ── W3 AI workspace  │         (depends on W1 context quality)
        W6 Frontend intel │
v2.3  ── W7 Plugin platform + marketplace
v2.x  ── W8 Remote dev, persistence, enterprise
```

W1 is first because completion/hover/diagnostics quality and the advanced refactorings all depend on real types — it's the difference between "good" and "PhpStorm-class." W2/W5 are independent product surfaces a second team can own in parallel. W3 depends on W1's context quality to be trustworthy.

## Guardrails (unchanged from v1)
Every release passes the **performance gate** (< 2 s start, < 500 MB idle, 60 fps, 1M files) and the **correctness golden-file suites**. Type inference and the debugger get their own fixture corpora (PHPStan/Psalm parity tests for W1; DBGp session fixtures for W4). Features that can't meet the budget ship behind a flag until they can.

## Definition of done for v2
A Laravel developer gets: type-accurate completion/refactoring, a debugger, a GitKraken-class visual git workflow with PRs, a first-class AI workspace, a premium database workspace, real frontend intelligence, and a plugin marketplace — all under the v1 performance envelope. At that point the pitch "**more modern and more productive than PhpStorm for Laravel**" is demonstrably true.
