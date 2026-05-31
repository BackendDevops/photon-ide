# 07 — Plugin SDK Specification

Extensibility is a first-class requirement: **sandboxed plugins, a marketplace, versioned APIs, hot reloading**, and the hard rule that **plugins must never block the main UI thread**. The SDK is also dogfooded — Photon's own Laravel intelligence, language packs, and DB drivers are built on it, which is the proof that the surface is sufficient.

## 1. Design principles

1. **Out-of-process by default.** Plugins do not share memory with the editor. A crashing or slow plugin is killed and restarted without affecting editing.
2. **Capability-based security.** A plugin gets *nothing* by default. It declares capabilities in its manifest; the user grants them; a broker in the core enforces them on every call.
3. **Async-only.** All host APIs are async and message-based over the same bus the UI uses. There is no synchronous API that could stall the UI.
4. **Versioned, stable contracts.** The API is semver'd; plugins declare the API range they support; the host guarantees compatibility within a major version.
5. **Same SDK for everyone.** First-party and third-party use identical contracts.

## 2. Runtime model

Two supported runtimes behind one host contract:

- **WASM (preferred):** plugins compiled to the WASM Component Model run in `wasmtime` with strict resource limits (memory, fuel/CPU, no ambient authority). Best isolation and startup; ideal for analyzers, extractors, formatters, linters. Authorable in Rust, AssemblyScript, TinyGo, etc.
- **Node (compat):** a restricted Node child process for the large existing JS tooling ecosystem (e.g. reusing VS Code-style language servers, prettier, eslint). Sandboxed via process isolation + a syscall/network broker; still cannot touch core memory or the UI thread.

```
core (capability broker) ─┬─ wasmtime instance  (plugin A)   [mem cap, fuel cap]
                          ├─ wasmtime instance  (plugin B)
                          └─ node child process (plugin C)   [rlimits, net broker]
        every call is an async, permission-checked bus message
```

LSP-based language plugins are a special, well-supported case: declare an LSP server binary/args + file types and the host manages the process and protocol bridging.

## 3. Manifest & capabilities

```jsonc
// photon.plugin.json
{
  "id": "acme.livewire-pack",
  "name": "Livewire Intelligence",
  "version": "1.4.0",
  "engine": "wasm",                 // wasm | node | lsp
  "api": "^1.2",                    // host API range
  "entry": "dist/plugin.wasm",
  "activation": ["onLanguage:php", "onLaravelProject", "onCommand:livewire.make"],
  "capabilities": {
    "index.extractors": ["php", "blade"],   // contribute index facts
    "language.providers": ["completion", "definition", "hover", "diagnostics"],
    "commands": ["livewire.make", "livewire.discover"],
    "ui.views": ["sidebar.livewire"],
    "fs.read": ["${workspace}/**"],          // scoped, glob-limited
    "fs.write": ["${workspace}/app/Livewire/**"],
    "process.spawn": ["php artisan *"],      // explicit allowlist
    "net": ["https://api.acme.dev"]          // explicit allowlist, else denied
  },
  "contributes": {
    "settings": "schema/settings.json",
    "themes": [], "keybindings": []
  }
}
```

The broker denies anything not declared. `fs`, `process.spawn`, and `net` are scoped allowlists, surfaced to the user at install/grant time in plain language ("This plugin wants to run `php artisan` and read your project files").

## 4. Extension points (the API surface)

Plugins extend Photon by registering providers/contributions. Major points:

| Area | Extension point | Example |
|---|---|---|
| Indexing | `Extractor` | Add facts for a framework (Filament, Livewire) |
| Language | `CompletionProvider`, `DefinitionProvider`, `HoverProvider`, `DiagnosticProvider`, `CodeActionProvider`, `SemanticTokensProvider` | New language or refinement |
| Types | `DynamicReturnTypeProvider`, `VirtualMemberProvider` | Teach the type engine about magic |
| Navigation | `SearchProvider` | New Search Everywhere category |
| Refactoring | `Refactoring` | New safe transformation |
| UI | `SidebarView`, `Panel`, `StatusItem`, `CodeLensProvider`, `Webview` | Custom panels (rendered in a sandboxed iframe/webview) |
| Commands | `Command` | Actions invocable from palette/keybinding |
| DB | `SqlDriver` | New database engine |
| Debug | `DebugAdapter` | New debug target |
| AI | `AiProvider`, `AiTool` | New model provider or agent tool |
| Themes/keymaps | declarative | Look & feel |

```rust
// Illustrative host trait a WASM plugin implements (via generated bindings)
trait CompletionProvider {
    fn languages(&self) -> Vec<Lang>;
    async fn complete(&self, doc: DocSnapshot, pos: Position, ctx: CompletionCtx)
        -> Vec<CompletionItem>;
}
```

```ts
// Illustrative TS-side SDK (for node/webview plugins)
import { photon } from '@photon/sdk';
photon.languages.registerCompletionProvider('php', {
  async provide(doc, pos) { return [{ label: 'dispatch', kind: 'method' }]; }
});
photon.commands.register('livewire.make', async (args) => { /* ... */ });
```

The SDK ships generated bindings (Rust + TypeScript) from the same `photon-ipc` contracts the core uses, so plugin APIs can never drift from the host.

## 5. UI contributions without UI-thread risk

Plugin views render in a **sandboxed webview/iframe** with their own context; they communicate via async messages and have no direct access to the editor DOM or the main React tree. A misbehaving plugin view can be slow *in its own frame* but cannot jank the editor. Native-feeling views are encouraged via a provided component kit that matches the active theme tokens.

## 6. Performance & resource governance

- **Per-plugin budgets:** memory cap, CPU/fuel cap, and a request-latency watchdog. Exceeding budget → throttle, then warn, then disable with a clear message.
- **Activation events:** plugins are inert until an activation event fires (`onLanguage`, `onCommand`, `onLaravelProject`), so installed-but-unused plugins cost nothing at startup — protecting the < 2 s / < 500 MB targets.
- **Backpressure:** plugin responses are cancellable; stale requests (user moved on) are dropped.
- **Isolation accounting:** the Photon Doctor panel shows per-plugin memory/CPU and lets the user disable a heavy plugin.

## 7. Hot reloading

For development and for seamless updates:
- A plugin in "dev mode" is watched; on rebuild, the host **tears down the old instance and spins up the new one** without restarting Photon — providers re-register, contributed views reload, in-flight requests are cancelled. State is intentionally not preserved across reloads (plugins persist their own state via a provided KV API if needed).
- Marketplace updates apply the same swap, so users update plugins without an IDE restart.

## 8. Marketplace

- **Registry** of signed plugin packages with metadata, versions, capability summaries, ratings, and download stats.
- **Signing & provenance:** packages are signed; the client verifies signatures. Capability diffs are shown on update ("now also requests network access").
- **Review pipeline:** automated scanning (declared vs used capabilities, known-bad patterns) plus human review for featured/verified status. Capability-based sandboxing means even unreviewed plugins are contained.
- **Distribution:** versioned, with `api` compatibility resolved at install; incompatible plugins are flagged rather than silently broken.
- **Private/enterprise registries:** organizations can host internal marketplaces and pin/allowlist plugins (ties into [15](./15-monetization.md) enterprise tier).

## 9. Versioning & compatibility policy

- Host API is semver. Within a major version, additive only; no breaking changes.
- Plugins declare `"api": "^1.2"`; the host refuses to load plugins requiring a newer major and warns for deprecated APIs with a migration window.
- Deprecations are announced with a documented replacement and a minimum of one minor-version overlap.

## 10. Security summary

Defense in depth: process/WASM isolation → capability broker → scoped allowlists → signing → marketplace review → runtime resource governance → per-plugin kill switch. The user is always in control of what a plugin can touch, and the editor's responsiveness is never at a plugin's mercy.

→ Next: [10 — AI Subsystem Architecture](./10-ai-subsystem.md)
