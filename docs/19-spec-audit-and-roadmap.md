# 19 — Spec Audit & Engine-First Roadmap

Audit of the PhpStorm-intelligence / VS-Code-lightness product spec against what
Photon ships today, with a prioritized backlog. **Priority order set by product:
intelligence engines first; Xdebug and Blade are sequenced last.**

Legend: ✅ done · 🟡 partial · ❌ missing

---

## 1. Intelligent core & static analysis

| Feature | Status | Notes |
|---|---|---|
| Laravel zero-config awareness | ✅ | Routes, Eloquent models + columns/relations, container bindings, events/jobs, config & i18n keys, middleware — all indexed, no extension needed. |
| Blade component awareness | ❌ | No `<x-…>`, slot/prop, `@extends/@include`, in-view `$var` typing. **Deferred (last).** |
| Symfony zero-config awareness | ❌ | No services.yaml/attributes/routing/DI understanding. |
| Advanced type inference | 🟡 | Declared member-type engine + chain resolver (`$this->svc->find()->`), Eloquent builder. Missing: PHPDoc **generics** (`Collection<User>`), union/intersection/nullable, conditional/dynamic returns, magic `@method`/`@property`, attribute extraction. |
| Dead-code / missing-return / contradictions | 🟡 | Inspection engine exists (unused/dup imports, debug calls, undefined `$this->`/`$var->` member, unknown route/config/i18n keys, unimported class). Missing: unreachable/dead code, missing return type, type mismatch, invalid override, unused private member, undefined function. |
| Safe global rename | ✅ | Project-wide rename updating classes/methods/vars + string references (plan→preview→apply, per-file reindex). Bind PhpStorm’s ⇧F6 alongside F2. |
| Deep navigation (Cmd+Click, vendor) | 🟡 | Symbol-resolved go-to-def, vendor declaration index. Missing: **core PHP stdlib + framework stubs** (built-in functions/classes), Go-to-Declaration vs Type split, inline peek. **Find Usages list still name-based** (go-to-def is resolved). |

## 2. Built-in power tools

| Feature | Status | Notes |
|---|---|---|
| Embedded DB client (SQL) | ✅ | MySQL/Postgres/SQLite: connect, schema browse, query, inline cell edit. |
| NoSQL (Mongo/Redis) | ❌ | Not supported. |
| SQL completion inside PHP strings | ❌ | No schema-aware SQL autocomplete in string literals. |
| Zero-config Xdebug | ❌ | No DBGp debugger. **Deferred (last).** |
| Local history (timeline recovery) | ❌ | No Git-independent per-edit snapshot/rollback. (We have a persistent index + fs-watcher, but no history store.) |

## 3. Core engine & performance

| Feature | Status | Notes |
|---|---|---|
| Sub-second startup | ✅ | Tauri 2 + Rust core + OS WebView; native, no JVM. |
| Async background indexing | 🟡 | Vendor index deferred/declaration-only/bulk; per-file reindex on save; **persistent warm-start** + fs-watcher. Indexing still holds the engine mutex — move heavy passes to a true worker thread. |
| Ultra-low memory (<500 MB idle) | ✅ | SQLite-backed index (no full in-RAM graph); status bar shows live RAM. |

## 4. Modern UX / UI

| Feature | Status | Notes |
|---|---|---|
| Minimalist borderless dark | ✅ | Fleet/Linear-style layered surfaces, no blue VS-Code status bar. |
| Omni-search (Search Everywhere) | ✅ | Files, classes, symbols, routes, IDE actions, settings. |
| Contextual status pills | ✅ | Laravel pill, PHP version, RAM, git, AI state, Photon mark. |

---

## Gap backlog — engine-first ordering

**Phase E1 — Type lattice (highest leverage)**
1. PHPDoc parsing: generics (`Collection<User>`, `array<int,Foo>`), union/intersection, nullable.
2. Magic members from `@method` / `@property` class docblocks.
3. PHP 8 attribute extraction; conditional/dynamic return refinement.
4. Eloquent return refinement: `Model::query()->get()` → `Collection<Model>`, `find()` → `?Model`.

**Phase E2 — Inspection engine expansion**
5. Unreachable/dead code after `return`/`throw`/`exit`.
6. Missing return type on declared methods/functions.
7. Type-mismatch & invalid-override (uses E1 types).
8. Unused private member; undefined free function (vs stdlib stubs).

**Phase E3 — Navigation & resolution**
9. **Core PHP stdlib + Laravel/Illuminate stubs** index (built-in functions/classes, facade members).
10. Symbol-resolved **Find Usages list** (receiver-scoped, not name-based).
11. Go-to-Declaration vs Go-to-Type; inline quick-definition peek.

**Phase E4 — Refactor & completion depth**
12. Change Signature; Move Class / Namespace.
13. Named-argument completion; `composer.json` PSR-4 autoload map for resolution.

**Phase P — Power tools (after engines)**
14. SQL completion inside PHP string literals (schema-aware).
15. NoSQL clients (Redis, MongoDB).
16. Local History (timeline snapshots + rollback).

**Phase Perf**
17. Background indexing on a dedicated worker thread (never hold the engine lock).
18. Commit-graph virtualization to 100k+; optional gitoxide backend.

**Deferred to last (explicit product decision)**
19. Symfony zero-config support.
20. Xdebug zero-config debugger.
21. Blade intelligence (components/slots/directives/in-view typing).

---

## PHP language support (target: through 8.5, current latest)

- **Version detection is dynamic** — `detect_php_version` reads the runtime
  `PHP_VERSION` (or the composer `php` constraint), so 8.5 is reported with no
  cap. The status pill reflects whatever the project/runtime uses.
- **Modern keyword awareness** (8.0–8.5) is in completion: `never/void/mixed/
  iterable/object/callable`, `readonly`, `enum`, `match`, `fn`, property-hook
  `get`/`set`, etc.
- **Full 8.3/8.4/8.5 syntax indexing** (property hooks, asymmetric visibility,
  typed class constants, `#[\Override]`, new-in-initializers) needs a
  **tree-sitter-php grammar bump** — tracked as a backlog task; the parser is
  error-tolerant so older grammars still index the bulk of modern files.

## PHP 8.x static-analysis feature matrix (senior-dev parity)

Most 8.0–8.2 constructs already parse with the current grammar, so analysis can
be layered now; 8.4 property hooks / asymmetric visibility need the grammar bump.

| # | Feature | Status | Plan |
|---|---|---|---|
| 1 | Attributes `#[Attr(...)]` — completion of attribute classes; param type-check | ❌ | `#[` triggers attribute-class completion; param typing after E1 union work. |
| 2 | Constructor promotion — resolution / refactor-to-promote | 🟡 | Resolution ✅ (promoted props indexed). “Convert to promotion” quick-fix → E4. |
| 3 | Enums — `->value`/`->name`, `::from()/::tryFrom()`; `match` exhaustiveness | ❌ | Enum-aware member types + **match-arm exhaustiveness warning** (next E2). |
| 4 | Union / intersection / DNF types `(A&B)|C` | 🟡 | Parser handles them; engine takes first member today. Full lattice → E1. |
| 5 | `readonly` props/classes — reassignment after construct | ✅ | **Shipped**: error on `$this->prop = …` outside `__construct`. |
| 6 | `#[Override]` (8.3) — must actually override a parent method | ❌ | Cross-file check via supertypes+members in `lint_file` (E3). |
| 7 | `match` return type + nullsafe `?->` null-propagation | ❌ | Needs flow types — E1 lattice + nullable propagation. |

Architecture note (per product guidance): the analysis core is already Rust and
runs **off the UI thread** (Tauri commands). The roadmap’s worker-thread
indexing item removes the last lock contention; a full LSP surface remains an
option if Photon ever hosts external editors.

## Sequencing rationale
The type lattice (E1) is the multiplier: accurate generics/union types make
completion, inspections, and navigation all sharper, so it leads. Inspections
(E2) and resolution/stubs (E3) compound on it. Power tools and the two large,
self-contained subsystems (Xdebug, Blade) follow once the analysis core is deep.
