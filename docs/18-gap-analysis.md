# 18 — Photon vs PhpStorm: Gap Analysis & Implementation Plan

> Brutally honest, grounded in **what Photon's code actually does today** (not the aspirational prose in docs/01–10). Where an earlier doc promised a feature the engine doesn't yet implement, this report says so.

---

## Executive Summary

Photon today is a **fast, native, Laravel-aware code navigator and editor** with a genuinely differentiated feature set in four areas — **startup/footprint, Laravel intelligence breadth, integrated Git graph, and an AI workspace** — but it is **not yet a semantic peer of PhpStorm** in the two areas that define a "smart" PHP IDE: **type inference** and **whole-program correctness (inspections/debugging/testing)**.

The honest position:

- **Where Photon already wins:** cold-start and memory (native Tauri/Rust vs JVM), multi-root + cross-project navigation in one window, Laravel breadth (routes, Eloquent incl. migration-derived columns + builder + scopes, config, i18n, container, events, jobs, env, validation, middleware, request-input completion), 20+ code generators, a GitKraken-style commit graph, and a built-in AI chat. Several of these match or exceed **Laravel Idea** already.
- **Where Photon is materially behind:** the **type engine is pragmatic, not real** (no PHPDoc generics, union/intersection, dynamic return types, flow analysis), so completion/inspections are heuristic; there is **no debugger, no test runner, no real inspection engine, no Blade component intelligence, and vendor code is not indexed** (so you can't navigate into framework/package classes). These are the things a PhpStorm user feels within five minutes.

**Strategic verdict:** Do **not** chase PhpStorm feature-for-feature. The winning wedge is **"PhpStorm-class correctness on the Laravel happy-path, at VS Code speed, AI-native."** That means: invest hard in the **type engine (the keystone)** and **vendor indexing** (they unlock completion, inspections, and navigation simultaneously), ship a **debugger and PHPUnit/Pest runner** for credibility, deepen **Blade**, and treat AI as the multiplier that lets a small team leapfrog JetBrains' 15-year head start on classic static analysis.

---

## Feature Gap Matrix

Legend — Photon: ✅ solid · 🟡 partial/heuristic · ⬜ none. Gap is the work to reach competitive parity.

| Feature | PhpStorm | Photon | Gap | Priority | Phase |
|---|---|---|---|---|---|
| **Parsing / symbol index** | full | ✅ tree-sitter + SQLite, ranges | small | — | done |
| Type inference (native types) | full | 🟡 `$this`/`new X`/typed params | large | Critical | v1.5 |
| PHPDoc generics / arrays | full | ⬜ | large | High | v2 |
| Union / intersection types | full | ⬜ | medium | Medium | v2 |
| Dynamic return types | full | 🟡 (Eloquent builder hard-coded) | large | High | v1.5 |
| Attributes (PHP 8) | full | ⬜ (not extracted) | medium | Medium | v1.5 |
| Magic methods/properties | full (`@method`/`@property`) | ⬜ | medium | High | v2 |
| Go to Definition | full | ✅ (name + cross-project) | small | — | done |
| Go to Declaration/Type/Impl | full | ⬜ (definition only) | medium | High | v1.5 |
| Quick definition popup | full | 🟡 (hover card) | small | Medium | v1.5 |
| Recent files / locations | full | ⬜ | small | Medium | v1 |
| Find Usages | full (semantic) | 🟡 (name-based + popup) | medium | High | v1.5 |
| Interface/trait/impl usages | full (graph) | 🟡 (by name) | medium | High | v1.5 |
| Search Everywhere | full | 🟡 (files/symbols/routes) | medium | High | v1 |
| — Actions / Settings / DB objects | full | ⬜ | medium | Medium | v1.5 |
| Rename | full | ✅ (name-based + preview) | small | — | done |
| Extract Variable/Method, Inline | full | ✅ (heuristic) | small | — | done |
| Safe Delete | full | ✅ (range-based) | small | — | done |
| Change Signature | full | ⬜ | large | Medium | v2 |
| Move Class / Namespace | full | ⬜ | large | Medium | v2 |
| Inspections (unused/undef/type) | full | 🟡 (Laravel-key + unimported class) | large | Critical | v1.5 |
| Quick-fixes | full | 🟡 (import class) | medium | High | v1.5 |
| Autocomplete (class/member) | full | 🟡 (index + pragmatic type) | medium | High | v1.5 |
| Named-argument completion | full | ⬜ | small | Low | v2 |
| Composer / autoload awareness | full | ⬜ | medium | High | v1 |
| **Vendor code indexing/nav** | full | ⬜ (vendor skipped) | medium | Critical | v1 |
| PHPUnit / Pest runner | full | ⬜ (artisan only) | large | High | v1.5 |
| Coverage | full | ⬜ | medium | Low | v2 |
| **Debugger (Xdebug)** | full | ⬜ | large | High | v1.5 |
| Laravel: routes | Idea-class | ✅ | small | — | done |
| Laravel: Eloquent (cols/rel/scopes/builder) | Idea-class | ✅ | small | — | done |
| Laravel: config / env / i18n | Idea-class | ✅ | small | — | done |
| Laravel: container bindings + nav | Idea-class | ✅ | small | — | done |
| Laravel: events / queues | Idea-class | 🟡 (indexed + panel) | small | Medium | v1.5 |
| **Laravel: Blade components/slots/props** | Idea-class | ⬜ | medium | High | v1.5 |
| Code generators | Idea-class | ✅ 20+ templates | small | — | done |
| Git graph / diff / blame / conflicts | strong | ✅ | small | — | done |
| Database tools | strong (DataGrip) | 🟡 (connect/schema/query/edit) | medium | Medium | v2 |
| AI assistant | plugin | ✅ native chat (BYO-key) | — | — | done |
| AI fixes / tests / agent | plugin | ⬜ | medium | High | v2 |
| Startup time / memory | heavy (JVM) | ✅ native | — | **win** | done |

---

## Per-Category Analysis

For each: *PhpStorm → Photon → gap → value → complexity → maintenance → architecture → priority/phase.*

### 1. PHP Semantic Engine — **the keystone**
- **PhpStorm:** full inference incl. PHPDoc generics (`@template`, `Collection<int,User>`), union/intersection, dynamic/conditional returns, flow narrowing, attributes, magic members via `@method`/`@property`.
- **Photon:** tree-sitter parse + symbol/reference index with byte ranges (real and fast). Inference is **pragmatic**: enclosing class for `$this`/`self`, `$x = new Foo`, typed params/properties, plus **hard-coded Eloquent builder** awareness. No generics, no union/intersection resolution, no flow analysis, **attributes not extracted**, no magic-member modeling.
- **Gap / value:** Large but the **highest-leverage gap** — completion accuracy, inspections, and type navigation all depend on it. This is the single biggest lever on "feels smart."
- **Complexity:** High. **Maintenance:** High (PHP's dynamism + framework magic).
- **Architecture:** A real type lattice over the existing AST: (1) resolve names via composer autoload + `use` map; (2) declared types; (3) PHPDoc layer (PHPStan/Psalm-compatible syntax — reuse their fixtures); (4) bounded flow inference per visible method, cached by revision; (5) **dynamic-return-type provider** trait so the Laravel engine teaches the core (Eloquent, `app()`) instead of hard-coding. Confidence-scored, degrade-gracefully (no false "undefined" on dynamic code).
- **Priority: Critical · Phase: v1.5 (foundation now, generics/magic in v2).**

### 2. Navigation Engine
- **PhpStorm:** Definition / Declaration / Type / Implementation / Quick-definition / Recent files & locations.
- **Photon:** Go-to-Definition by name (cross-project ✅), Cmd+click → usages popup, hover card. Missing: Declaration vs Definition split, Go-to-Type, Go-to-Implementation (needs the relation graph), Recent Files/Locations, peek/quick-definition.
- **Value:** High (daily). **Complexity:** Low–Medium. **Maintenance:** Low.
- **Architecture:** Store `extends/implements/uses_trait` edges in a `symbol_relations` table (cheap during extraction) → powers Go-to-Implementation and interface/trait usages. Recent files/locations = an in-memory ring + session persist. Quick-definition = a peek overlay reading the def's range.
- **Priority: High · Phase: v1 (recents) / v1.5 (impl + type).**

### 3. Search Everywhere
- **PhpStorm:** files, classes, methods, symbols, **actions**, **settings**, DB objects, (+ plugins: routes/models).
- **Photon:** files + symbols + routes with a CamelHumps fuzzy ranker, streamed. Missing: **Actions** (run commands), **Settings**, **Database objects**, dedicated Models category.
- **Value:** High — it's the IDE's front door and a stated "surpass PhpStorm" goal. **Complexity:** Medium. **Maintenance:** Low.
- **Architecture:** Provider trait (`SearchProvider`) already implied; add Action/Settings/DbObject/Model providers, merge-rank by `fuzzy × recency × kind-weight`, stream top-K. DB objects already in the schema index — just expose a provider.
- **Priority: High · Phase: v1 (Actions/Settings) / v1.5 (DB/Models).**

### 4. Find Usages
- **PhpStorm:** semantic, resolves the exact symbol; interface/trait/enum usages via the reference graph.
- **Photon:** name-based references (fast, good UX with the popup) but **not symbol-resolved** — same-named members across classes collide; interface implementations are approximate.
- **Value:** High (trust). **Complexity:** Medium. **Maintenance:** Medium.
- **Architecture:** Resolve references to a `symbol_id` during the index resolve-phase (we already store unresolved names). Add member-receiver typing (depends on §1) to disambiguate `$x->save()`. Incremental: re-resolve only references touching changed names.
- **Priority: High · Phase: v1.5.**

### 5. Refactoring Engine
- **Photon has:** Rename, Extract Variable/Method, Inline Variable, Safe Delete (range-based) — all plan/preview/apply. **Missing:** Change Signature, Move Class/Namespace.
- **Value:** Medium (the present set covers ~80% of daily use). **Complexity:** Change Signature = High (call-site rewriting + types), Move = High (namespace + `use` fixups across project). **Risk:** silent breakage — mitigated by the existing plan→preview→apply pattern + flagging uncertain edits.
- **Architecture:** Both need the resolved reference graph (§1/§4). Move Class = file move + namespace edit + update every `use`/FQN + composer autoload check.
- **Priority: Medium · Phase: v2.**

### 6. Inspections / Code Correctness — **second keystone**
- **PhpStorm:** unused vars/imports, dead code, missing return types, type mismatches, invalid overrides, undefined methods/properties, hundreds more.
- **Photon:** unknown route/config/translation key + unimported-class diagnostics only. **No real inspection engine.**
- **Value:** Critical for credibility — this is what makes an IDE feel "intelligent." **Complexity:** High (depends on §1). **Maintenance:** High.
- **Architecture:** A debounced background analyzer producing `Diagnostic`s per file off the edit pipeline; an `Inspection` trait (so each rule is pluggable and individually toggleable); reuse the type engine. Start with the cheap, high-signal ones (unused import/var, undefined symbol, missing return) before type-mismatch. Pair each with a quick-fix (`CodeAction`).
- **Priority: Critical · Phase: v1.5.**

### 7. Autocomplete
- **Photon has:** class names, pragmatic member completion, keywords/snippets, and a **strong Laravel layer** (route/config/env/trans/middleware/request-input/validation/Eloquent builder+columns+scopes). **Missing:** named-argument completion, accuracy on inferred types (depends on §1), constructor-promotion awareness.
- **Value:** High. **Complexity:** Medium (mostly rides §1). **Maintenance:** Medium.
- **Architecture:** Already provider-based in Monaco; precision improves automatically as §1 lands. Add named-arg completion from method signatures (cheap once signatures are indexed). AI ghost-text as a parallel provider (BYO-key).
- **Priority: High · Phase: v1.5.**

### 8. Composer & Vendor — **cheap, high-impact, currently broken**
- **PhpStorm:** composer.json/autoload awareness, vendor navigation, dependency intelligence.
- **Photon:** **vendor is explicitly skipped during indexing** → you cannot go-to-definition into `Illuminate\…` or any package, and FQN resolution ignores composer PSR-4. This is the most-felt missing piece after the type engine.
- **Value:** Critical (navigation/completion into the framework). **Complexity:** Medium. **Maintenance:** Low.
- **Architecture:** Parse `composer.json` autoload (PSR-4/classmap) → FQN↔path map. **Index vendor at declaration-level fidelity** (classes/methods/signatures, skip method bodies/references) so memory stays bounded — the existing "tiered fidelity" idea, now actually implemented. Lazy: index a package on first navigation if not pre-indexed.
- **Priority: Critical · Phase: v1.**

### 9. PHPUnit / Pest
- **Photon:** none (can run via the Artisan/terminal). **PhpStorm:** discovery, runner, filtering, coverage, failure navigation.
- **Value:** High (TDD credibility). **Complexity:** Medium. **Maintenance:** Medium.
- **Architecture:** Discover tests from `tests/` + `@test`/`it()`; run via `vendor/bin/phpunit|pest --teamcity` (machine-readable) in a PTY; parse results → inline gutter pass/fail, a results tree, click-to-failure. Coverage later via `--coverage-clover`.
- **Priority: High · Phase: v1.5.**

### 10. Debugger (Xdebug)
- **Photon:** none. **PhpStorm:** full DBGp.
- **Value:** High — a real IDE debugs. **Complexity:** High (DBGp protocol, async state). **Maintenance:** Medium.
- **Architecture:** A Rust **DBGp listener** (TCP) ↔ a DAP-shaped UI (breakpoints, step, stack, scopes, watches, evaluate). Laravel-aware launch presets ("Debug artisan command", "Debug current test"). Reuse a DAP client for JS/TS later.
- **Priority: High · Phase: v1.5.**

### 11. Laravel Intelligence — **the moat (already strong)**
- **Routing/Eloquent/Config/i18n/Container:** ✅ at or near Laravel-Idea level, including **migration-derived columns, dynamic `whereX`, scopes, builder chains, binding navigation** — genuinely competitive today.
- **Gaps:** **Blade** (component `<x-…>` nav, slots, props, directive intelligence, `$var` typing in views) — ⬜, the biggest Laravel gap. **Events/queues** indexed but inline dispatch→listener/job nav is partial. Runtime reflection (`route:list`, container) is not used (static only).
- **Value:** High (Blade is everywhere). **Complexity:** Medium (Blade) / Low (event-dispatch nav). **Maintenance:** Medium.
- **Architecture:** Index Blade views (name, component class/view, `@props`, slots) → completion+nav; tree-sitter-blade or a tolerant Blade lexer; type `$variables` passed from controllers where statically resolvable. Optional **opt-in runtime reflection** (a small artisan helper) to harden routes/bindings.
- **Priority: High (Blade) · Phase: v1.5.**

### 12. AI-Native Opportunities — **the leapfrog**
- **Photon has:** a native, BYO-key, project-context chat. **Missing:** the things that beat JetBrains rather than match it.
- **Where AI changes the game per category:**
  - **Inspections/fixes:** LLM-proposed fixes for diagnostics → apply via the existing ChangeSet path (safe, previewable). Covers the long tail static analysis can't.
  - **Refactoring:** "convert array → DTO", "extract service", "add type hints" — emit ChangeSets.
  - **Tests:** generate a Pest/PHPUnit test for the method under cursor, using the real type/Eloquent context.
  - **Navigation/search:** natural-language "where do we charge the card?" over the symbol+route index.
  - **Debugging:** explain a stack trace / failing test using the live context.
  - **Commit/PR:** already heuristic; upgrade to LLM summaries.
- **Why it matters:** AI lets a small team **skip** building PhpStorm's hundreds of hand-written inspections and instead deliver *fix quality* with grounded context. This is the strategic differentiator — not chat, but **AI wired into the safe edit path**.
- **Priority: High · Phase: v2 (after the type/inspection foundation makes context trustworthy).**

---

## Architecture Recommendations (consolidated)

1. **Make the type engine real and provider-driven** (§1). Everything smart depends on it. Dynamic-return-type providers let `laravel` teach `php` without coupling.
2. **Index vendor at declaration fidelity + parse composer autoload** (§8). Unlocks navigation/completion into the framework with bounded memory.
3. **Resolve references to symbol ids** (§4) → trustworthy Find Usages + Go-to-Implementation + enables Move/Change-Signature.
4. **Add `symbol_relations`** (extends/implements/uses) during extraction — cheap, unlocks several nav features.
5. **Background diagnostics framework** with a pluggable `Inspection` trait + paired `CodeAction` quick-fixes (§6), reusing the type engine and the debounced edit pipeline.
6. **DBGp listener** and a **TeamCity-protocol test runner** as standalone services behind the command bus (§9/§10) — isolatable, restartable.
7. **AI behind the ChangeSet apply path** (§12) so every AI edit is previewed/undoable like a refactor.
8. **Hold the performance gate**: vendor indexing, type inference, and inspections must run on the low-priority pool, paged to SQLite, cancellable — or they erode the one durable advantage (speed/footprint).

---

## Roadmap

### MVP (done)
Native shell, parse/index, navigation by name, Search Everywhere (files/symbols/routes), Laravel breadth, Git, terminal, templates, AI chat, design system. **Status: shipped in the current build.**

### v1 — close the cheap, high-impact gaps
- **Composer autoload + vendor declaration indexing** (navigate into the framework). *(Critical)*
- **Recent Files / Recent Locations.**
- **Search Everywhere: Actions + Settings providers.**
- **`symbol_relations` table** (groundwork for impl/usages).
- Index persistence / warm-start + filesystem watcher (already designed, finish it).

### v1.5 — competitive parity ("feels like a real IDE")
- **Type engine v1** (native types + PHPDoc basics + flow narrowing + dynamic-return providers). *(Critical)*
- **Inspection engine** (unused import/var, undefined symbol, missing return) + quick-fixes. *(Critical)*
- **Find Usages resolved to symbols**; Go-to-Implementation / Declaration / Type.
- **Debugger (Xdebug DBGp).**
- **PHPUnit/Pest runner** with inline results + failure nav.
- **Blade intelligence** (components/slots/props/directives, view `$var` typing).
- Quick-definition peek; named-argument completion.

### v2 — differentiators ("better than PhpStorm for Laravel")
- **PHPDoc generics, union/intersection, magic members, attributes** (full type lattice).
- **Change Signature, Move Class/Namespace.**
- **AI wired into fixes/refactors/tests/debugging** (grounded, ChangeSet-safe). *(the leapfrog)*
- DataGrip-class DB workspace (visual query builder, ER diagrams), coverage.
- GitKraken-class extras (drag-drop ops, interactive rebase, PR integration).

### Future
- Remote/SSH/container dev; plugin marketplace (real WASM runtime); team intelligence; web surface.

---

## Technical Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| **Type engine quality** never reaching "trustworthy" | Medium | High | Follow PHPStan/Psalm semantics + reuse their fixtures; confidence-gate; allow external LSP fallback; golden-file CI. |
| **Performance erosion** from vendor index + inference + inspections | Medium | High (kills the core advantage) | Low-priority pool, SQLite-paged, cancellable, per-subsystem budgets; perf gate in CI blocks regressions. |
| Vendor indexing **memory blow-up** | Medium | Medium | Declaration-level fidelity only; lazy per-package; LRU eviction. |
| Debugger protocol correctness (DBGp) | Medium | Medium | Fixture-based session tests; ship behind a flag; isolate as a service. |
| AI **incorrect edits** / cost | Medium | Medium | Edits via preview/apply ChangeSet; BYO-key; ground in type+Laravel context; opt-in. |
| Monaco limits on huge files / custom rendering | Low–Medium | Medium | Rust-owned rope + viewport feed; documented escape hatch to custom renderer. |
| **Scope overload** (chasing all of PhpStorm) | High | Medium | This report's priorities: type engine + vendor + inspections + debugger + Blade first; everything else AI-assisted or deferred. |
| Tauri/WebView fragmentation | Medium | Medium | Cross-WebView CI; conservative web baseline. |

---

## Final Recommendation

**Do not copy PhpStorm. Beat it on a narrower front.** The prioritized plan that maximizes developer value while preserving Photon's philosophy (lightweight · fast · Laravel-first · AI-native · modern · extensible):

1. **v1 — fix the two embarrassing gaps cheaply:** **index vendor + composer autoload** (so the framework is navigable) and add **Recent Files/Locations + Search Everywhere Actions**. Low effort, instantly closes "this can't even open Illuminate."
2. **v1.5 — earn "real IDE" status:** land the **type engine v1**, a **background inspection engine with quick-fixes**, **symbol-resolved Find Usages + Go-to-Implementation**, the **Xdebug debugger**, a **Pest/PHPUnit runner**, and **Blade intelligence**. This is the make-or-break release — it converts "fast Laravel navigator" into "IDE I can replace PhpStorm with."
3. **v2 — leapfrog:** complete the **type lattice** (generics/magic/attributes) and, crucially, **wire AI into the safe edit path** (fixes, refactors, test generation, debugging explanations). This is where Photon stops matching JetBrains and starts being *more* productive — delivering fix/refactor quality without a 15-year backlog of hand-written inspections.
4. **Always:** keep the **performance gate** sacred. The moment Photon is as slow and heavy as PhpStorm, it has no reason to exist. Every smart feature ships on the background pool, paged to disk, cancellable — or it ships behind a flag until it can.

**One-line strategy:** *PhpStorm-class correctness on the Laravel happy-path, at VS Code speed, with AI doing the work JetBrains needed a decade and a thousand inspections to do.*
