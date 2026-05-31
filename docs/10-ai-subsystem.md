# 10 — AI Subsystem Architecture

AI is a first-class citizen, not a bolt-on. Photon provides a **chat panel, inline completion, agent mode, project-wide context, code explanation, and refactoring suggestions**, across **pluggable providers (Claude, OpenAI, Gemini, local LLMs)**. The differentiator is not the model — it's the **Laravel/PHP-aware context engine** that feeds the model, and the **safe edit path** that applies its changes.

## 1. Architecture overview

```
┌─────────────────────────────────────────────────────────────┐
│ ai orchestrator                                              │
│  ┌────────────┐  ┌──────────────┐  ┌──────────────────────┐ │
│  │ Chat       │  │ Inline       │  │ Agent runtime         │ │
│  │ sessions   │  │ completion   │  │ (plan→tool→observe)   │ │
│  └─────┬──────┘  └──────┬───────┘  └──────────┬───────────┘ │
│        └────────────────┴──────────────┬──────┘             │
│                    Context Engine ◄─────┘                    │
│        (index + php + laravel + editor selection + git)      │
│                          │                                   │
│                  Provider Layer (trait)                      │
│   ┌─────────┬─────────┬─────────┬───────────────────────┐    │
│   │ Claude  │ OpenAI  │ Gemini  │ Local (Ollama/llama.cpp│    │
│   └─────────┴─────────┴─────────┴───────────────────────┘    │
│                          │                                   │
│                   Edit Application                           │
│        (AI changes → ChangeSet → refactoring plan/apply)     │
└─────────────────────────────────────────────────────────────┘
```

The orchestrator lives in `photon-ai` (Rust). The chat/agent UI is React ([08](./08-ui-architecture.md)). Everything is async and streamed.

## 2. Provider layer (pluggable, BYO-key, privacy-first)

```rust
#[async_trait]
trait AiProvider {
    fn id(&self) -> &str;                  // "claude" | "openai" | "gemini" | "local:..."
    fn capabilities(&self) -> Caps;        // streaming, tools, vision, ctx window, fim
    async fn complete(&self, req: CompletionReq) -> TokenStream;
    async fn chat(&self, req: ChatReq) -> TokenStream;      // supports tool calls
    async fn embed(&self, texts: Vec<String>) -> Vec<Vec<f32>>; // optional
}
```

- **Built-in providers:** Claude, OpenAI, Gemini, and a **local** provider (Ollama / llama.cpp / LM Studio endpoints) for fully offline/private use.
- **Plugin providers:** new providers register via the SDK ([07](./07-plugin-sdk.md)) `AiProvider` extension point.
- **Keys & privacy:** API keys stored in the OS keychain. A clear privacy posture: which provider, what gets sent, and a **local-only mode** for regulated environments. Per-project policy can restrict providers (enterprise).
- **Routing:** users pick a default model per task (fast model for inline completion, strong model for agent/chat); fallback chains on error/rate-limit.

## 3. The Context Engine (the real moat)

Generic AI assistants paste the open file and hope. Photon's context engine assembles **precise, relevant** context from the same intelligence that powers navigation:

- **Symbol-graph retrieval:** from the cursor/selection, pull the definitions of referenced symbols, the types involved (from [05](./05-php-analysis-engine.md)), and their signatures — so the model sees the *actual* `User` model, not a guess.
- **Laravel awareness:** include relevant routes, model relations/columns, container bindings, config, and Blade components ([06](./06-laravel-intelligence.md)) when they're germane to the request. Ask "add an endpoint to update a user's avatar" and the agent already knows your routing conventions, the `User` model's columns, and your storage config.
- **Repo retrieval:** a hybrid of (a) symbol/index lookup and (b) optional local embeddings over the codebase (computed incrementally, stored alongside the index) for semantic "find similar code" recall. Embeddings can use a local model to avoid sending code out.
- **Recency & edits:** recently edited files, current diff (git), and the current diagnostics are included so the model fixes the *current* problem.
- **Budgeting & ranking:** context is ranked and trimmed to fit the model's window, prioritizing exact symbol defs > Laravel facts > nearby code > broad retrieval. Token budget is explicit and shown.

This is why Photon's AI answers are correct about *your* project where generic tools hallucinate.

## 4. Inline completion

- **FIM (fill-in-the-middle)** style completion with prefix/suffix and lightweight symbol context.
- **Fast path:** a small/fast model (or local) for low-latency ghost-text; debounced, cancellable on keystroke, never blocking typing (same cancellation discipline as the rest of the IDE).
- **Acceptance UX:** Tab to accept, word-by-word accept, multi-line with diff preview for larger suggestions.
- **Quality gates:** completions are syntax-checked (tree-sitter) and type-sanity-checked before display where cheap; obviously broken suggestions are suppressed.

## 5. Chat panel

- Threaded conversations scoped to the project; messages can `@`-reference files, symbols, routes, models, selections, the current diff, or "whole project."
- Streamed responses with rich rendering: code blocks get **Apply** (→ ChangeSet preview/apply), **Insert**, **Copy**, and **diff view** actions.
- Code explanation: select code → "Explain" produces an explanation grounded in resolved types/relations, not surface reading.
- Conversations persist locally; no server-side history unless the provider requires it (disclosed).

## 6. Agent mode

The agent can take multi-step actions to accomplish a goal, with the user in control.

- **Loop:** plan → call a tool → observe result → continue, streaming its reasoning/steps to the panel.
- **Tools (capability-gated):** read file, search symbols/usages, run a refactoring, edit files (via ChangeSet), run a terminal command (allowlisted, confirmed), run tests, query the DB (read-only by default), inspect routes/models. Tools are the same operations the IDE exposes, so the agent acts *through* the IDE's safe APIs.
- **Safety:** every edit is a **ChangeSet** that goes through the refactoring plan/apply path ([02](./02-module-design.md) `refactoring`) — previewed as a diff, applied atomically, undoable as one unit. Destructive actions (run command, write files, DB writes) require explicit confirmation by default; an "auto-approve within scope" mode is opt-in with a clear boundary.
- **Tool extensibility:** plugins contribute `AiTool`s ([07](./07-plugin-sdk.md)); MCP-style external tools can be wired in.
- **Checkpoints:** the agent snapshots workspace state before a run so the user can revert the entire run in one action.

## 7. Refactoring & code-action suggestions

- AI-suggested code actions appear alongside engine-provided ones (extract, rename, fix). Because they emit ChangeSets validated against the live index, an AI rename is as safe as a manual one.
- "Suggest improvements" on a file/selection yields reviewable, applyable diffs, never silent edits.

## 8. Performance & resource discipline

- AI never runs on the UI thread; all calls are async streams with cancellation.
- The context engine reuses already-computed index/semantic data — assembling context is cheap because the IDE already knows the codebase.
- Embeddings (if enabled) are computed incrementally in the background pool at low priority and stored on disk; they respect the same memory budget rules ([09](./09-tauri-rust-backend.md)).
- Local-model mode keeps everything on-device for users who can't send code externally.

## 9. Privacy, cost & control

- **Transparency:** a per-request indicator of which provider/model and approximate tokens; a log of what context was sent (auditable).
- **Cost controls:** per-feature model selection, token budgets, and usage metering; warnings before large agent runs.
- **Data policy:** BYO-key means usage is governed by the user's own provider account; Photon adds no hidden retention. Enterprise policy can force local-only or a specific approved provider.
- **Opt-in:** AI features are fully optional; the IDE is excellent with AI entirely disabled.

## 10. Why this is differentiated

Anyone can call an LLM. Photon's advantage is that the **context comes from a real PHP/Laravel semantic model**, the **edits flow through a verified refactoring engine**, and the whole thing is **provider-agnostic and privacy-respecting**. The AI is as good as its context and as safe as its apply path — and Photon owns both.

→ Next: [11 — MVP Roadmap](./11-mvp-roadmap.md)
