# 13 — Scaling Strategy

"Scaling" for a desktop IDE has three distinct axes: **scaling to huge codebases** (performance), **scaling the product/codebase** (engineering org), and **scaling the business infrastructure** (the services around the app). This document addresses all three.

## 1. Performance scaling — to 1M+ files and beyond

The architecture is built so resource use tracks **active work**, not project size. The levers:

- **Disk-backed index, paged into memory.** The symbol/intelligence store is SQLite, memory-mapped, with bounded caches ([03](./03-database-schema.md), [09](./09-tauri-rust-backend.md)). A 1M-file project's index is GBs on disk but only the hot working set is resident. Resident memory stays in the hundreds of MB.
- **Lazy, prioritized indexing.** Never block on a full scan; index open/visible/source first, vendor/generated last ([04](./04-indexing-engine.md)). The editor is usable in seconds regardless of repo size.
- **Incremental everything.** Per-file deltas, incremental tree-sitter reparse, two-phase resolution touching only affected names. Steady-state edit cost is O(one file), not O(project).
- **Parallelism with politeness.** Rayon fan-out for extraction at low OS QoS; single batched SQLite writer to avoid contention; cancellation drops stale work. Scales with cores without starving the foreground.
- **Tiered fidelity.** Vendor/generated code indexed at declaration-level by default; full reference graph optional. Keeps the common case fast.
- **Degradation, not failure.** Under memory pressure the watchdog evicts caches and reduces fidelity with a visible status, rather than OOMing. The IDE always stays responsive.
- **Benchmark as a gate.** A standard 1M-file synthetic repo + several large OSS Laravel repos run in CI; startup, warm-start, edit latency, and memory must pass or the build is blocked. Performance regressions are bugs caught before merge.

**Beyond a single machine — remote/monorepo scaling (v2.x):** the `workspace` VFS abstraction ([02](./02-module-design.md)) lets the index and engines run *near the files* — on a remote host, in a container, or on a dev server — with the UI local. For giant monorepos this means the heavy lifting happens where the code lives, and the laptop only renders. Same core, different deployment.

## 2. Engineering / codebase scaling — to a large team

- **Module boundaries as team boundaries.** The 16 modules ([02](./02-module-design.md)) each have an owning team, an explicit contract, and a private implementation. Teams ship independently behind stable interfaces. CI enforces no-cycles and no-cross-internal-deps so the architecture doesn't erode under headcount.
- **Contract-first with codegen.** `photon-ipc` is the single source of truth for UI↔core contracts, codegenerating TS types — eliminating a whole class of integration bugs and letting UI and core teams move in parallel.
- **The SDK is the internal API.** First-party intelligence (Laravel, languages, DB drivers) is built on the public plugin SDK ([07](./07-plugin-sdk.md)). This forces the extension surface to be good and lets new language/framework packs be built by separate teams (or the community) without touching the core.
- **Testing scales with the product:** golden-file correctness suites for intelligence (reviewable as diffs), the performance gate, and the cross-WebView matrix. New contributors get fast, trustworthy feedback.
- **Release safety:** feature flags, staged rollouts via the updater, beta/nightly channels, and crash/telemetry (opt-in) to catch regressions before they reach the stable channel.

## 3. Infrastructure / business scaling

The app is local-first, so the backend is intentionally thin — which is great for margins and reliability.

- **Update & distribution:** signed/notarized artifacts on a CDN; the Tauri updater pulls deltas. Scales trivially (static hosting).
- **Marketplace:** a registry service (metadata, packages, signing, search, ratings) — the main stateful service. Packages on object storage + CDN; the API is read-heavy and cacheable. Private/enterprise registries are tenant-scoped instances of the same service.
- **Licensing/accounts:** license issuance, seat management, billing — a modest service; offline-tolerant license validation so the IDE works without a constant connection.
- **AI:** **BYO-key by default** means inference cost is the user's, not ours — this is the single biggest infra-cost de-risk. An optional first-party AI offering (managed keys/quota) would introduce variable cost, handled by metering, per-tier quotas, model routing (cheap models for completion), and caching; priced to protect margin ([15](./15-monetization.md)).
- **Telemetry/crash (opt-in):** standard ingestion pipeline; aggregate-only; respects enterprise opt-out.
- **Embeddings (if first-party):** prefer on-device/local embeddings to avoid server cost and address privacy; a server option only for teams that want shared semantic search.

### Cost posture
Because compute lives on the user's machine and AI is BYO-key, marginal cost per user is dominated by CDN bandwidth and a small set of light services. This keeps gross margin high and lets the business scale users far faster than infra cost — a deliberate, structural advantage over server-heavy "cloud IDE" models.

## 4. What we explicitly refuse to do to scale
- We don't move the editor to the cloud "for scale" — it would break the latency promise that is the product.
- We don't grow memory with project size — disk-backed index instead.
- We don't add features that can't pass the performance gate — they ship behind a flag until they can.
- We don't centralize what can stay local — privacy and margin both benefit.

→ Next: [14 — Technical Risk Analysis](./14-risk-analysis.md)
