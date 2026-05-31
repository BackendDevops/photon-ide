# 14 — Technical Risk Analysis

An honest accounting of what could sink this product, scored by **likelihood × impact**, with concrete mitigations and early-warning signals. Risks are grouped: existential (could kill the product), serious (could cripple a release), and manageable.

Scoring: L/M/H for likelihood and impact.

## Existential risks

### R1 — PHP/Laravel intelligence isn't good enough to be trusted
**Likelihood: M · Impact: H.** This is the whole product. If go-to-definition is wrong, completion is noisy, or Eloquent magic isn't understood, developers won't switch — "almost right" navigation is worse than none. Building a PHP semantic engine to PhpStorm/PHPStan quality is genuinely hard (PHP's dynamism, generics-via-PHPDoc, framework magic).

*Mitigations:* (1) Reuse proven modeling — follow PHPStan/Psalm PHPDoc semantics so well-annotated code works for free, and validate against their fixtures. (2) Golden-file correctness suites over real OSS Laravel repos, run in CI; regressions are blocking. (3) **Honest confidence:** mark dynamic/uncertain results rather than emitting false errors — degrade gracefully. (4) Allow an external LSP (Intelephense/Phpactor) as a fallback/supplement behind the same contract ([05](./05-php-analysis-engine.md)) so we're never strictly worse than the status quo. (5) Phase Laravel features by impact ([11](./11-mvp-roadmap.md)) and dogfood relentlessly.
*Early warning:* correctness-suite pass rate per release; beta-user reports of wrong navigation.

### R2 — Performance targets erode as features land
**Likelihood: M · Impact: H.** It's easy to be fast with an empty editor; the targets (< 2 s, < 500 MB, 60 fps, 1M files) are hard to *hold* as DB tools, AI, debugger, and plugins arrive. Death by a thousand cuts.

*Mitigations:* (1) **Performance is a CI gate from day one** — startup, memory, edit latency, and the 1M-file benchmark must pass or the build is blocked ([09](./09-tauri-rust-backend.md), [12](./12-production-roadmap.md)). (2) Per-subsystem memory budgets + watchdog. (3) Lazy activation — unused subsystems/plugins cost nothing. (4) Architectural discipline: nothing heavy on the UI thread, everything cancellable, index on disk. (5) Features that can't meet budget ship behind a flag until they can.
*Early warning:* the gate trend line; any flag-gated feature lingering too long.

### R3 — Tauri / OS-WebView fragmentation & maturity
**Likelihood: M · Impact: M-H.** Three different WebView engines (WKWebView, WebView2, WebKitGTK) diverge in features and bugs; Tauri is younger than Electron. Editor rendering or floating-panel behavior could differ per OS, or we could hit a WebView wall (e.g., huge-file rendering).

*Mitigations:* (1) Cross-WebView CI matrix runs the UI suite on all three; divergence caught early. (2) Target a conservative baseline web feature set; polyfill/avoid bleeding-edge APIs. (3) The `EditorSurface` contract allows swapping Monaco's renderer for a custom Canvas/WebGL surface if a WebView proves inadequate ([08](./08-ui-architecture.md)) — the heavy logic is in Rust regardless. (4) Keep the WebView's job small (view-models only), reducing exposure to engine quirks.
*Early warning:* per-OS bug rate; frame-time deltas across WebViews.

## Serious risks

### R4 — Monaco's limits (large files, deep customization)
**Likelihood: M · Impact: M.** Monaco is JS-based and can struggle with very large files or fully custom rendering needs (sticky lines/semantic features are fine; multi-MB files and exotic decorations less so).
*Mitigations:* Rust-owned rope model with viewport feeding (Monaco never holds the whole large file); virtualization; the documented escape hatch to a custom renderer behind the same contract. Treat Monaco as replaceable from the start.

### R5 — Refactoring correctness (silent breakage)
**Likelihood: M · Impact: M-H.** A rename or move that misses a dynamic reference silently breaks code — catastrophic for trust.
*Mitigations:* Plan/apply with full diff preview; refactorings operate on semantic facts, not text; **uncertain/dynamic references flagged for user confirmation** rather than silently edited; atomic apply with single-step undo ([02](./02-module-design.md)). Ship rename first, broaden only as confidence proves out.

### R6 — Scope/resource overload (out-built by incumbents)
**Likelihood: H · Impact: M.** The full vision (IDE + DB tools + debugger + git + AI + plugins + marketplace) is enormous; a small team can spread too thin and ship a shallow everything that beats nothing.
*Mitigations:* Ruthless MVP scoping ([11](./11-mvp-roadmap.md)) — depth on editor + PHP + top Laravel features; defer DB/debugger/marketplace. Win a beachhead (Laravel devs) before broadening. The module architecture lets later areas be added (even by community/plugins) without rework.

### R7 — Database-tools surface area
**Likelihood: M · Impact: M.** Five DB engines, schema explorer, ER diagrams, EXPLAIN, editing — each engine is a long tail of dialect quirks.
*Mitigations:* `SqlDriver` trait + per-engine crates ([02](./02-module-design.md)); ship MySQL/Postgres/SQLite first (the Laravel-common ones), MariaDB/SQL Server later; lean on `sqlx`; treat drivers as pluggable so the community can fill gaps.

### R8 — AI quality, cost, and trust
**Likelihood: M · Impact: M.** Hallucinated edits, runaway cost, or privacy concerns could make AI a liability rather than an asset.
*Mitigations:* Context engine grounded in the real semantic model; AI edits flow through the verified ChangeSet path with preview/confirm; **BYO-key** so cost is the user's; local-model mode for privacy; transparency on what's sent ([10](./10-ai-subsystem.md)). AI is optional — the IDE is excellent without it.

## Manageable risks

### R9 — Plugin security & stability
**Likelihood: M · Impact: M.** Malicious or buggy plugins could exfiltrate code or jank the IDE.
*Mitigations:* Out-of-process + WASM/capability sandbox, scoped allowlists, signing, marketplace review, per-plugin resource caps and kill switch, never on the UI thread ([07](./07-plugin-sdk.md)).

### R10 — Cross-platform packaging, signing, notarization
**Likelihood: M · Impact: L-M.** Code signing/notarization (esp. macOS) and auto-update are fiddly and break silently.
*Mitigations:* Automate in `xtask`/CI from day one; test the update path each release; staged rollouts.

### R11 — Talent concentration (deep Rust + PHP-semantics + IDE expertise)
**Likelihood: M · Impact: M.** The skill set (Rust systems + language tooling + Laravel depth + IDE UX) is rare; key-person risk is real.
*Mitigations:* Strong docs (this bundle), contract-first modules to reduce coupling, golden-file tests that encode intent, and hiring against the module map so knowledge is distributed.

### R12 — Ecosystem moat erosion (incumbents move)
**Likelihood: M · Impact: M.** JetBrains could lighten PhpStorm; VS Code + a great Laravel extension + Copilot could close the gap; Cursor could add PHP depth.
*Mitigations:* Move fast on the Laravel-native + performance combination that's hard to retrofit onto a JVM IDE or a generic editor; make the integrated, native, fast experience the thing they can't easily copy. Community plugins deepen the moat.

## Risk heat summary

| ID | Risk | L | I | Tier |
|---|---|:--:|:--:|---|
| R1 | Intelligence quality | M | H | Existential |
| R2 | Performance erosion | M | H | Existential |
| R3 | Tauri/WebView fragmentation | M | M-H | Existential-adjacent |
| R4 | Monaco limits | M | M | Serious |
| R5 | Refactoring correctness | M | M-H | Serious |
| R6 | Scope overload | H | M | Serious |
| R7 | DB tools surface | M | M | Serious |
| R8 | AI quality/cost/trust | M | M | Serious |
| R9 | Plugin security | M | M | Manageable |
| R10 | Packaging/signing | M | L-M | Manageable |
| R11 | Talent concentration | M | M | Manageable |
| R12 | Incumbent response | M | M | Manageable |

## The two things to get right
If R1 (intelligence trust) and R2 (sustained performance) are nailed, the rest are execution. Both are mitigated by the same discipline: **automated gates that block regressions** — correctness golden-files for R1, the performance benchmark for R2. Everything else follows from there.

→ Next: [15 — Monetization Strategy](./15-monetization.md)
