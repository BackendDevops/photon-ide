# 16 — Git Experience (Flagship)

Photon's Git experience is a **flagship feature**, not an IDE afterthought. It is explicitly **not** modeled on VS Code, SourceTree, or traditional IDE SCM panels. The reference point is **GitKraken**: a visual, interactive, drag-and-drop graph where a power user runs an entire workflow without ever opening a terminal.

This document specifies the full vision, then states precisely what is **implemented in v1 today** versus what is **designed and scheduled**.

## Core philosophy

Every Git operation must be **visual, intuitive, discoverable, safe, and fast**. The commit graph is the center of gravity; the terminal is a fallback you rarely need.

## Performance targets

| Operation | Target |
|---|---|
| Repository open | < 1 s |
| Commit graph render | < 100 ms |
| Branch switching | near-instant |
| Graph updates | real-time |
| Repos with millions of objects | stays responsive |

These are met by computing the graph topology in Rust (lane assignment is O(commits)), streaming commits in pages, and **virtualizing** graph rows so only the visible window paints. The graph never renders 100k DOM nodes — it renders the viewport.

---

## Feature areas

### 1. Visual commit graph
Real-time interactive graph: branch/merge/rebase/detached-HEAD/tag visualization, remote branches, author avatars, commit metadata preview, commit search and filtering. Lanes are colored and stable; merges render with multi-parent edges.

### 2. Drag-and-drop Git operations
Direct manipulation with a **visual preview before execution** for every action:
- Drag commit → branch (move/rebase)
- Drag branch → branch (merge)
- Drag branch → commit (reset)
- Drag commit (cherry-pick)
- Interactive rebase via drag.

### 3. Interactive rebase UI
A dedicated **timeline-based** rebase editor (not a text file): reorder, squash, fixup, drop, edit, and split commits, with a live preview of the resulting history.

### 4. Advanced branch management
Branch grouping and folders, health indicators, ahead/behind visualization, activity tracking, stale-branch detection, and cleanup recommendations — beyond what PhpStorm offers.

### 5. Pull request integration
Native GitHub / GitLab / Bitbucket / Azure DevOps: create, review, inline comments, approve, request changes, view CI/CD status, and review diffs — without leaving the IDE.

### 6. Smart diff viewer
Side-by-side and inline, word-level diffing, syntax-aware and **PHP/Laravel-aware** diffing (semantic, using the engine in docs/05–06), plus image diffs, JSON diffs, and migration diffs.

### 7. Commit workspace
GitKraken-style workspace: staged/unstaged lists, **hunk and line staging**, file history, commit preview, related-issue linking, and **AI-generated commit messages**.

### 8. Repository insights
Analytics most IDEs lack: top contributors, hotspot files, frequent conflict zones, commit velocity, branch activity, code ownership, technical-debt indicators.

### 9. Conflict resolution center
A dedicated three-way merge UI: side-by-side comparison, **AI-assisted resolution**, one-click accept options, and batch resolution — dramatically better than a text merge editor.

### 10. AI-assisted Git workflows
Deep AI integration (via docs/10): commit message generation, PR summaries, release notes, conflict explanation and resolution suggestions, branch-cleanup recommendations, and pre-merge risk analysis.

---

## Architecture

```
UI (React)                         Core (Rust, src-tauri/src/git.rs)
─────────────────────────────────  ──────────────────────────────────────────
CommitGraph  (SVG lanes, virtual)  graph(limit)  → GraphCommit[] with lanes
GitSidebar   (commit workspace)    status / stage / unstage / commit
RebaseTimeline (planned)           branches / checkout / create_branch
ConflictCenter (planned)           diff / log / push / pull / stash
PrPanel (planned)                  suggest_commit_message (heuristic → AI)
                                   [planned] graph drag-ops, rebase plan,
                                             PR providers, insights, conflicts
```

**Lane assignment** (`assign_lanes`) is the standard active-lanes sweep over commits newest→oldest: each lane reserves the hash it expects next; a commit takes the lane reserved for it (or a free lane), then that lane reserves its first parent while extra parents (merges) open new lanes. The frontend maps `hash → (row, lane)` and draws bezier edges to each parent. This is O(commits) and renders as an SVG the UI can virtualize.

The backend uses the system `git` binary today (robust, complete); the design's preferred **gitoxide (`gix`)** backend is a drop-in behind the same command surface for the < 1 s open / millions-of-objects targets.

---

## Status — implemented in v1 vs. planned

### ✅ Implemented now
- **Visual commit graph** — colored lanes, merge nodes, author-initial avatars, ref/tag/HEAD chips, commit metadata, click-to-select. Rendered as SVG from Rust-computed lanes (`git_graph`).
- **Commit workspace** — staged/unstaged lists, stage/unstage (per-file and all), branch switcher, push/pull/stash, commit box.
- **AI-style commit messages** — `git_suggest_message` generates a message from the staged changes (deterministic heuristic in v1; upgrades to the LLM path in docs/10 with no UI change).
- **Diff viewer** — unified diff with +/- and hunk coloring (click a changed file).
- **Branch management basics** — list, switch, create, ahead/behind readout.

### 🔜 Designed & scheduled (this doc is the spec)
- **Drag-and-drop operations** with visual preview (move/merge/reset/cherry-pick/rebase). Backend primitives partly exist (checkout, cherry-pick via CLI); the interaction layer + preview engine are next.
- **Interactive rebase timeline** UI (reorder/squash/fixup/drop/edit/split).
- **Pull request integration** (GitHub/GitLab/Bitbucket/Azure) — provider adapters behind one `PrProvider` trait, mirroring the SqlDriver pattern (docs/02).
- **Smart diff** — side-by-side, word-level, PHP/Laravel-aware, image/JSON/migration diffs.
- **Repository insights** — contributors, hotspots, conflict zones, velocity, ownership.
- **Conflict resolution center** — three-way merge UI with AI assistance and batch resolve.
- **Graph virtualization to 100k+ commits** and the gix backend for the performance targets.

### Why this sequencing
The graph + commit workspace + AI messages are the daily-use core and the most visible GitKraken-like differentiator, so they ship first. Drag-ops, the rebase timeline, PRs, smart diff, insights, and the conflict center are each substantial sub-products; they build on the same `git.rs` command surface and the graph already in place.

---

## Competitive goal
The Git experience should become **one of the primary reasons** developers choose Photon over PhpStorm, GitKraken, SourceTree, VS Code, and Cursor — a complete, visual, AI-assisted workflow with no terminal required. v1 lays the foundation (graph + workspace); the roadmap above is how it gets all the way there.
