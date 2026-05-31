import { useState } from "react";
import BranchMenu from "./BranchMenu";
import { api, type ProjectSummary } from "../lib/api";

// Command Center (Arc / Raycast / Fleet inspired): workspace + branch on the
// left, a prominent Search Everywhere field in the centre, and live status +
// Git actions on the right.
export default function Header({
  projectName,
  branch,
  summary,
  indexing,
  onOpenFolder,
  onSearch,
  onSettings,
  onGitAction,
  onCheckout,
  onNewBranch,
  onToast,
}: {
  projectName: string | null;
  branch: string | null;
  summary: ProjectSummary | null;
  indexing: boolean;
  onOpenFolder: () => void;
  onSearch: () => void;
  onSettings: () => void;
  onGitAction: (a: "update" | "commit" | "push") => void;
  onCheckout: (branch: string) => void;
  onNewBranch: () => void;
  onToast: (msg: string) => void;
}) {
  const [branchMenu, setBranchMenu] = useState(false);

  const run = async (fn: () => Promise<string>, ok: string) => {
    try {
      await fn();
      onToast(ok);
    } catch (e) {
      onToast(String(e));
    }
  };

  return (
    <div className="h-11 shrink-0 flex items-center gap-2 px-2.5 surface-2 border-b border-line">
      {/* left cluster: workspace + branch */}
      <div className="flex items-center gap-1 rounded-lg bg-white/[0.03] p-0.5">
        <button onClick={onOpenFolder} className="seg" title="Open folder">
          <span className="text-accent text-xs">◆</span>
          <span className="text-fg font-medium truncate max-w-[150px]">
            {projectName || "Open Folder…"}
          </span>
          <span className="text-fg-faint text-2xs">▾</span>
        </button>
        {branch && (
          <button onClick={() => setBranchMenu((v) => !v)} className="seg" title="Branches">
            <span className="text-running">⎇</span>
            <span className="truncate max-w-[150px]">{branch}</span>
            <span className="text-fg-faint text-2xs">▾</span>
          </button>
        )}
      </div>

      {/* centre: Search Everywhere */}
      <div className="flex-1 flex justify-center px-2">
        <button
          onClick={onSearch}
          className="group flex items-center gap-2 w-full max-w-md h-7 px-3 rounded-lg bg-surface-1 border border-line hover:border-line-strong text-fg-faint hover:text-fg-muted transition-colors duration-120"
          title="Search Everywhere"
        >
          <span className="text-sm">⌕</span>
          <span className="text-sm flex-1 text-left">Search files, symbols, routes…</span>
          <span className="kbd group-hover:border-accent/40">⇧⇧</span>
        </button>
      </div>

      {/* right cluster: status + git + settings */}
      <div className="flex items-center gap-1.5">
        {summary?.is_laravel && (
          <span className="chip bg-[#f55247]/15 text-[#ff7a6e]" title="Laravel project">
            Laravel
          </span>
        )}
        <span
          className="chip bg-white/[0.04] text-fg-muted"
          title={indexing ? "Indexing project" : "Index ready"}
        >
          <span
            className="dot"
            style={{ background: indexing ? "var(--warn,#f0b429)" : "#3fd07e" }}
          />
          {indexing ? "indexing" : "ready"}
        </span>
        <span className="chip bg-ai/12 text-ai" title="AI assistant (idle)">
          <span className="dot bg-ai animate-pulse2" />
          AI
        </span>

        {branch && (
          <div className="flex items-center gap-0.5 ml-1 pl-1.5 border-l border-line">
            <button className="icon-btn" title="Update Project (pull)" onClick={() => run(api.gitUpdate, "Project updated")}>↙</button>
            <button className="icon-btn" title="Commit" onClick={() => onGitAction("commit")}>◦</button>
            <button className="icon-btn" title="Push" onClick={() => run(api.gitPush, "Pushed")}>↗</button>
          </div>
        )}
        <button className="icon-btn text-accent" title="Run (roadmap)" onClick={() => onToast("Run/Debug: docs/12")}>▷</button>
        <button className="icon-btn" title="Settings (⌘,)" onClick={onSettings}>⚙</button>
      </div>

      {branchMenu && (
        <BranchMenu
          onClose={() => setBranchMenu(false)}
          onAction={(a) => {
            if (a === "new-branch") onNewBranch();
            else if (a === "update") run(api.gitUpdate, "Project updated");
            else onGitAction(a);
          }}
          onCheckout={onCheckout}
        />
      )}
    </div>
  );
}
