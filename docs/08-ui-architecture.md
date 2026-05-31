# 08 — UI Component Architecture

The UI must feel like **PhpStorm Ultimate's depth, Fleet's calm, Cursor's AI fluency, and Raycast's command speed** — on a **dark, compact, minimal-chrome** surface with floating panels and smooth animation, while never feeling like an Electron app. This document covers the component tree, state management, the editor surface, virtualization, theming, and the performance guardrails that keep it native-feeling.

## 1. Design tenets

1. **Chrome is earned.** Default to a near-bezel-less editor. Panels appear when invoked and recede when not. The most-used action (Search Everywhere) is one keystroke (double-shift), not a visible toolbar.
2. **The UI is a view, not a brain.** Every component renders a view-model pushed from the Rust core and emits intents. No business logic, no project model, no parsing in JS. This is the single most important rule for both correctness and memory.
3. **60 fps or it's a bug.** Animations are GPU-composited transforms/opacity only. Lists virtualize. The editor paints the viewport plus a small overscan, never the whole file.
4. **Keyboard-first, mouse-complete.** Everything is reachable by keyboard; nothing *requires* the mouse. PhpStorm and VS Code keymaps ship as presets.

## 2. Shell layout

```
┌───────────────────────────────────────────────────────────────────────┐
│  Title/Activity strip (project ⌄ · branch ⌄ · run ▷ · AI ✦ · search ⇧⇧) │
├───┬───────────────────────────────────────────────────────────┬────────┤
│ A │  Primary sidebar (files / structure / git / db / search)  │  AI    │
│ c │ ┌───────────────────────────────────────────────────────┐ │  panel │
│ t │ │ Breadcrumbs · sticky lines                            │ │ (chat/ │
│ i │ │                                                       │ │ agent) │
│ v │ │            Editor group (tabs, splits)         minimap│ │  ✦     │
│ i │ │                                                       │ │        │
│ t │ │ Inline hints · code lens · diagnostics gutter         │ │        │
│ y │ └───────────────────────────────────────────────────────┘ │        │
│ b │  Bottom dock: terminal / problems / debug / db results     │        │
├───┴───────────────────────────────────────────────────────────┴────────┤
│  Status bar: position · encoding · indexing ◐ · LSP ● · git ↑↓ · AI ●  │
└───────────────────────────────────────────────────────────────────────┘
```

- **Activity bar** is a thin icon rail (collapsible to nothing).
- **Panels are dockable and floatable.** A panel can pop out into a floating, translucent window (Fleet-style) — implemented as a portal layer above the editor with a blurred backdrop.
- **Command palette / Search Everywhere** renders as a centered floating surface (Raycast aesthetic): large input, grouped streamed results, inline preview pane.

## 3. Component tree

```
<App>
 ├─ <BusProvider>                  // single connection to Rust core; query cache
 │   ├─ <ThemeProvider>            // CSS variables from settings; no FOUC
 │   ├─ <KeymapProvider>           // PhpStorm/VSCode/custom keymaps → intents
 │   ├─ <Shell>
 │   │   ├─ <ActivityBar/>
 │   │   ├─ <SidebarHost>          // pluggable views, lazy-mounted
 │   │   │   ├─ <FileTreeView/>     // virtualized tree
 │   │   │   ├─ <StructureView/>
 │   │   │   ├─ <GitView/>
 │   │   │   ├─ <DbExplorerView/>
 │   │   │   └─ <SearchView/>
 │   │   ├─ <EditorArea>
 │   │   │   ├─ <EditorGroup>*      // splits
 │   │   │   │   ├─ <TabStrip/>
 │   │   │   │   ├─ <Breadcrumbs/>
 │   │   │   │   ├─ <StickyLines/>
 │   │   │   │   ├─ <EditorSurface/>   // Monaco-backed, Rust-driven model
 │   │   │   │   └─ <Minimap/>
 │   │   ├─ <BottomDock>
 │   │   │   ├─ <TerminalView/>     // xterm grid, PTY-backed
 │   │   │   ├─ <ProblemsView/>
 │   │   │   ├─ <DebugView/>
 │   │   │   └─ <DbResultsGrid/>     // virtualized, paged
 │   │   ├─ <AiPanel/>             // chat / agent run / diffs
 │   │   └─ <StatusBar/>
 │   └─ <OverlayLayer>             // portals: palette, floating panels, hints
 │       ├─ <SearchEverywhere/>
 │       ├─ <QuickActions/>
 │       └─ <FloatingPanelPortal/>
```

`*` repeated for splits. Sidebar/dock views are **lazy-mounted**: their JS and DOM only exist when the view is first opened, keeping the WebView heap small at startup.

## 4. State management

Three tiers, deliberately:

1. **Server state (the project) → query cache.** A TanStack-Query-style cache over the bus. View-models are fetched/subscribed, cached with revisions, and invalidated by core events. Components never own project truth.
2. **UI state (layout, panel sizes, active tab) → lightweight store** (Zustand). Persisted to the settings store so layout restores at T1 startup.
3. **Local component state → React `useState`.** Ephemeral (hover, focus, input drafts).

No global Redux mega-store; no business state in the UI. This keeps re-renders local and predictable.

```ts
// Illustrative: subscribing a view-model
const { data: tokens } = useBusSubscription(
  ['semanticTokens', docId, revision],
  () => bus.request({ kind: 'SemanticTokens', doc: docId }),
);
```

## 5. The editor surface

- **View layer:** Monaco, configured for the requested features — multi-cursor, **column (box) selection**, code folding, minimap, **sticky lines**, breadcrumbs, inline (inlay) hints, parameter hints, symbol highlighting, **semantic highlighting**, and **code lens**. Monaco gives these with mature UX out of the box.
- **Model bridge:** the authoritative document rope lives in Rust (`editor` module). Monaco is fed view content for the viewport; edits round-trip as `EditDocument{rev}` to the core, which is the source of truth. This is the key deviation from stock Monaco: we do **not** keep the whole large file as a JS string model, which is Monaco's scaling weakness.
- **Decorations** (diagnostics, semantic tokens, inlay hints, code lens, symbol highlights) arrive as streamed events and are applied as Monaco decorations, patched incrementally by revision.
- **Escape hatch:** if Monaco's renderer becomes the bottleneck on huge files, the `EditorSurface` contract allows swapping in a custom Canvas/WebGL renderer without changing the model or any other component. Decision gated on profiling, not taste.

## 6. Virtualization everywhere

The 1M-file target means *no* component may render an unbounded list:

- **File tree:** virtualized; children fetched on expand; flat-mode search results streamed.
- **Search Everywhere / Find Usages:** streamed + windowed; only visible rows mount.
- **DB result grid:** virtualized rows *and* columns; paged fetch from the driver; never materializes a full result set.
- **Terminal:** virtualized scrollback grid.
- **Problems/diagnostics:** windowed list grouped by file.

## 7. Theming

- **CSS custom properties** define the entire palette and metrics; themes are JSON files mapping tokens → values. Dark is the default and the design target; light and high-contrast ship too.
- Theme is loaded from the fast settings cache **before first paint** to avoid any flash.
- Semantic-token colors map to theme tokens so PHP/Blade/TS all share a coherent palette.
- Plugin-contributable themes go through the same token schema (validated), so they can't break layout.

## 8. Motion & "native feel"

- Animations use only `transform`/`opacity` (compositor-only) with short durations (120–200 ms) and ease-out curves. Panels slide/fade; the palette scales-in subtly.
- **Respect reduced-motion** OS setting.
- Native menus, native title bar integration, native file dialogs, and OS-correct scrollbars/trackpad behavior come via Tauri — this is a large part of why it doesn't feel like Electron.
- Window vibrancy/acrylic where the OS supports it for floating panels.

## 9. Accessibility & input

- Full keyboard navigation; focus rings; ARIA roles on all interactive surfaces.
- Screen-reader announcements for diagnostics and completion.
- Configurable font (incl. ligatures), line height, and density (compact/comfortable).

## 10. Performance guardrails (enforced)

- **Render budget telemetry:** a dev-mode overlay flags any frame > 16 ms and any component re-rendering more than N times per interaction.
- **WebView heap ceiling:** a watchdog warns in CI test runs if the renderer heap exceeds budget on the standard large-project benchmark.
- **No synchronous bus calls** from render; all data flows through the subscription cache.
- **Memo discipline:** list rows and editor decorations are memoized by stable keys + revision.

The UI's job is to be a beautiful, fast, honest window onto the Rust core — never a second source of truth.

→ Next: [09 — Tauri / Rust Backend Architecture](./09-tauri-rust-backend.md)
