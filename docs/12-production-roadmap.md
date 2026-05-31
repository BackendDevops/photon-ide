# 12 — Production Roadmap

From a credible MVP ([11](./11-mvp-roadmap.md)) to the **definitive PHP/Laravel IDE**. The sequencing principle: ship the features that (a) close the gap to PhpStorm parity and (b) deepen the Laravel moat, while never regressing the performance promise.

## Release train

Predictable cadence: monthly minor releases on a stable channel, plus a nightly/beta channel for early adopters. Every release passes the performance gate (startup/memory/large-project) or it doesn't ship.

## v1.0 — "Daily driver for Laravel" (MVP + must-haves)

Beyond MVP, 1.0 needs the things a pro can't live without:
- **Refactoring engine (full):** Rename (already), Extract Method/Variable, Inline Variable, Change Signature, Move Class/Namespace, Safe Delete — all plan/apply with preview, updating PHP + Blade references ([02](./02-module-design.md)).
- **Database tools v1:** connection manager (MySQL, PostgreSQL, SQLite), schema explorer, query runner with virtualized results, basic table editing. Eloquent↔schema linking turned on (column validation against real tables).
- **Debugger v1:** Xdebug (DBGp) — breakpoints, stepping, variables, watches, conditional breakpoints; Laravel-aware launch presets.
- **Git v1.5:** stash, branch compare, blame, interactive rebase (guided UI), cherry-pick, conflict resolution.
- **Type engine v2:** generics solver (collections/builders), dynamic return types, flow narrowing, array shapes — parity with PHPStan-annotated codebases.
- **Laravel breadth:** container/binding navigation, events/listeners, queues/jobs, factories/seeders, localization missing-key detection, full Blade prop/slot completion.
- **Exit:** an honest "90% of PhpStorm for Laravel" claim is defensible.

## v1.x — Ecosystem & breadth (quarters after 1.0)

- **Plugin SDK + Marketplace (public):** open the extension points used internally; signed packages, capability sandbox, hot reload ([07](./07-plugin-sdk.md)). Seed with first-party packs (Livewire, Filament, Inertia, Pest/PHPUnit, API tooling).
- **AI agent mode:** multi-step agent with safe ChangeSet edits, tool use, checkpoints ([10](./10-ai-subsystem.md)); local-model support hardened.
- **Frontend intelligence:** real Vue/React/TypeScript support (TS server integration + tree-sitter), Tailwind class completion, Inertia page↔component navigation, Blade↔Vue/React boundary awareness.
- **Database tools v2:** MariaDB + SQL Server drivers, EXPLAIN plan visualization, ER diagrams, data export/import, query history.
- **Testing:** PHPUnit/Pest runner with inline results, coverage gutters, "run/debug nearest test," failure navigation.
- **More databases & Redis:** Redis browser/inspector (the stack explicitly includes Redis), queue/cache inspection tying into Laravel queues.

## v2.0 — "Better than PhpStorm for Laravel" (year 2)

- **Runtime Laravel reflection** matured: opt-in app introspection for fully-resolved routes/bindings/discovered providers, cached and merged.
- **Deep refactorings:** move-with-namespace-fix across the project, pull-up/push-down members, introduce parameter object, convert array→DTO, etc.
- **Profiling & performance tools:** integrate with Xdebug profiler / Blackfire-style flamegraphs; N+1 query detection via Eloquent awareness.
- **HTTP client / API tooling:** REST client (the stack lists REST APIs) with environment vars, OpenAPI import, and route-aware request scaffolding.
- **Docker integration:** the stack lists Docker — container/service awareness, run/debug inside containers, Sail integration, remote interpreters.
- **Collaboration (optional):** lightweight live-share for pairing.

## v2.x+ — Platform & scale

- **Remote/SSH/container development:** swap the workspace VFS for a remote backend ([13](./13-scaling-strategy.md)); index runs near the files.
- **Web/cloud surface (optional):** a browser-hosted variant reusing the Rust core via WASM/remote — strategic optionality, not a near-term commitment.
- **Enterprise:** SSO/SAML, private marketplace, policy controls (AI provider restrictions, telemetry off), audit, fleet management ([15](./15-monetization.md)).
- **Team intelligence:** shared index/insights, codebase Q&A, onboarding assist.

## Capability maturity matrix

| Capability | MVP | v1.0 | v1.x | v2.0 |
|---|:--:|:--:|:--:|:--:|
| Editor + PHP intelligence | ✅ | ✅✅ | ✅✅ | ✅✅ |
| Laravel core (routes/eloquent/blade/config) | ✅ | ✅✅ | ✅✅ | ✅✅ |
| Laravel full (container/events/queues/jobs/i18n) | – | ✅ | ✅✅ | ✅✅ |
| Navigation + Search Everywhere | ✅ | ✅ | ✅ | ✅ |
| Refactoring (rename) | ✅ | – | – | – |
| Refactoring (full + deep) | – | ✅ | ✅ | ✅✅ |
| Database tools | – | ✅ | ✅✅ | ✅✅ |
| Debugger | – | ✅ | ✅ | ✅✅ |
| Git (full) | basic | ✅ | ✅ | ✅ |
| AI chat + completion | ✅ | ✅ | ✅ | ✅ |
| AI agent | – | – | ✅ | ✅✅ |
| Plugin SDK + marketplace | – | – | ✅ | ✅ |
| Frontend (Vue/React/TS) | basic | basic | ✅ | ✅✅ |
| Remote/containers | – | – | – | ✅ |

(✅ shipped/usable, ✅✅ best-in-class, – not yet.)

## Guardrails that never relax
Every release: performance gate green, correctness golden-files green, cross-platform/WebView matrix green. New features that can't meet the latency/memory budget ship disabled-by-default behind a flag until they can. The performance promise is the brand; it is never traded for features.

→ Next: [13 — Scaling Strategy](./13-scaling-strategy.md)
