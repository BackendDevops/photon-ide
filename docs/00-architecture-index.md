# Photon IDE — Architecture & Product Specification

> A lightweight, next-generation IDE for **PHP & Laravel** that delivers **90–95% of PhpStorm's developer experience** at a **fraction of the resource cost**.
>
> *PhpStorm intelligence · VS Code performance · Cursor UX · JetBrains polish · native desktop feel.*

---

## What this is

This bundle is the complete, venture-scale engineering and product specification for **Photon IDE** (working codename). It is organized as a documentation repo: each deliverable is a standalone file so it can be owned, reviewed, and versioned independently.

## Non-negotiable targets

| Metric | Target | Why it matters |
|---|---|---|
| Cold startup | **< 2 s** to interactive editor | The single most-felt difference vs. JVM IDEs |
| Warm startup | **< 600 ms** | Re-opening a known project must feel instant |
| Idle memory (medium project) | **< 500 MB** RSS | Leaves headroom for the rest of the dev machine |
| Large project | **1M+ files** indexed without UI stall | Monorepos, vendor trees, generated code |
| Keystroke-to-paint latency | **< 16 ms** (60 fps) | Typing must never feel laggy |
| Go-to-definition | **< 50 ms** p95 (warm index) | Navigation must feel "free" |
| Search Everywhere first results | **< 100 ms** | The command palette is the IDE's front door |

These numbers are the product. Every architectural decision in this bundle is justified against them.

## Core philosophy

1. **Performance is a feature, not a tuning pass.** The architecture is designed cold-start-first and memory-first. Nothing heavy runs on the UI thread, ever.
2. **Laravel-first.** Where a generic PHP IDE stops, Photon keeps going — routes, Eloquent, the container, Blade, config, queues are first-class domain objects, not regex heuristics.
3. **Intelligence over features.** A smaller set of deeply correct features beats a large set of shallow ones.
4. **Extensible by default.** Core capabilities (PHP intelligence, Laravel intelligence, DB tools) are themselves plugins built on the same SDK third parties use.
5. **Native feel, web velocity.** Tauri + Rust for the shell and engines; React + TypeScript for a UI that ships fast — but with strict guardrails so it never feels like Electron.

## Technology stack (at a glance)

- **Desktop runtime:** Tauri 2 (Rust core, OS WebView for UI — no bundled Chromium).
- **UI:** React + TypeScript + Tailwind, with a custom virtualized editor surface.
- **Backend services:** Rust workspace (multi-crate), async via Tokio.
- **Language intelligence:** Tree-sitter for syntax + an LSP host + a bespoke PHP semantic engine.
- **Persistence:** SQLite (symbol index, project intelligence, caches) + RocksDB-style append log for incremental updates.
- **AI:** Pluggable provider layer (Claude, OpenAI, Gemini, local LLMs) behind a stable internal contract.

See [09 — Tauri/Rust backend](./09-tauri-rust-backend.md) for the full rationale, including why **not** Electron and where the WebView boundary sits.

---

## Deliverables index

| # | Document | Covers |
|---|---|---|
| 01 | [System Architecture](./01-system-architecture.md) | Process model, layering, data flow, threading, IPC |
| 02 | [Domain-Driven Module Design](./02-module-design.md) | The 16 modules, bounded contexts, editor/nav/refactor/db/git/terminal |
| 03 | [Database Schema](./03-database-schema.md) | SQLite symbol & intelligence schema, migrations, indexes |
| 04 | [Indexing Engine](./04-indexing-engine.md) | Incremental indexing, file watching, scale to 1M+ files |
| 05 | [PHP Analysis Engine](./05-php-analysis-engine.md) | Type inference, generics, traits, attributes, PHPDoc |
| 06 | [Laravel Intelligence Engine](./06-laravel-intelligence.md) | Routes, Eloquent, container, Blade, config, queues |
| 07 | [Plugin SDK Specification](./07-plugin-sdk.md) | Sandboxing, versioned APIs, marketplace, hot reload |
| 08 | [UI Component Architecture](./08-ui-architecture.md) | Component tree, state, virtualization, theming, panels |
| 09 | [Tauri / Rust Backend Architecture](./09-tauri-rust-backend.md) | Crate layout, IPC, async runtime, memory strategy |
| 10 | [AI Subsystem Architecture](./10-ai-subsystem.md) | Providers, context engine, agent mode, inline completion |
| 11 | [MVP Roadmap](./11-mvp-roadmap.md) | What ships first, 0→1 milestones, definition of done |
| 12 | [Production Roadmap](./12-production-roadmap.md) | 1.0 → maturity, feature sequencing |
| 13 | [Scaling Strategy](./13-scaling-strategy.md) | Performance scaling, team scaling, infra scaling |
| 14 | [Technical Risk Analysis](./14-risk-analysis.md) | Top risks, likelihood/impact, mitigations |
| 15 | [Monetization Strategy](./15-monetization.md) | Pricing, packaging, GTM, unit economics |

## How to read this

- **Investors / leadership:** Start with this README, then [11 MVP](./11-mvp-roadmap.md), [15 Monetization](./15-monetization.md), [14 Risk](./14-risk-analysis.md).
- **Architects:** [01 System](./01-system-architecture.md) → [02 Modules](./02-module-design.md) → [09 Backend](./09-tauri-rust-backend.md).
- **Language tooling engineers:** [04 Indexing](./04-indexing-engine.md) → [05 PHP](./05-php-analysis-engine.md) → [06 Laravel](./06-laravel-intelligence.md) → [03 Schema](./03-database-schema.md).
- **Plugin authors:** [07 Plugin SDK](./07-plugin-sdk.md).

---

*Codename "Photon". This document is a design specification, not shipping code. Interface sketches in Rust/TypeScript are illustrative of contracts and shapes, not final signatures.*
