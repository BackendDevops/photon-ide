import { useEffect, useState } from "react";
import { api, type ProjectSummary } from "../lib/api";

export default function StatusBar({
  summary,
  activePath,
  dirty,
  indexing,
  branch,
  onToggleTerminal,
  onGitChanged,
  onToast,
}: {
  summary: ProjectSummary | null;
  activePath: string | null;
  dirty: boolean;
  indexing: boolean;
  branch: string | null;
  onToggleTerminal: () => void;
  onGitChanged?: () => void;
  onToast?: (msg: string) => void;
}) {
  const [stats, setStats] = useState<{ php_version: string; memory_mb: number } | null>(null);
  const [hub, setHub] = useState<{ branches: { name: string; current: boolean }[]; ahead: number; behind: number } | null>(null);

  const openHub = async () => {
    if (hub) {
      setHub(null);
      return;
    }
    try {
      const [branches, st] = await Promise.all([api.gitBranches(), api.gitStatus()]);
      setHub({ branches, ahead: st.ahead, behind: st.behind });
    } catch {
      setHub({ branches: [], ahead: 0, behind: 0 });
    }
  };
  const switchBranch = async (name: string) => {
    setHub(null);
    try {
      await api.gitCheckout(name);
      onGitChanged?.();
      onToast?.(`Switched to ${name}`);
    } catch (e) {
      onToast?.(`Checkout failed: ${String(e).split("\n")[0].slice(0, 80)}`);
    }
  };
  const openPr = async () => {
    setHub(null);
    try {
      await api.openExternal(await api.gitPrUrl());
      onToast?.("Opening pull request in browser…");
    } catch (e) {
      onToast?.(`PR failed: ${String(e).split("\n")[0].slice(0, 80)}`);
    }
  };
  useEffect(() => {
    let alive = true;
    const tick = () =>
      api.systemStats().then((s) => alive && setStats(s)).catch(() => {});
    tick();
    const id = setInterval(tick, 3000);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, []);

  const project = summary?.root ? summary.root.replace(/[\\/]+$/, "").split(/[\\/]/).pop() : null;

  return (
    <div className="h-7 shrink-0 flex items-center justify-between px-2 surface-2 border-t border-line text-2xs text-fg-muted">
      <div className="flex items-center gap-1.5">
        <button onClick={onToggleTerminal} className="seg !py-0.5 !px-2 text-2xs" title="Toggle Terminal (⌘`)">
          <span>▱</span>
          <span>Terminal</span>
        </button>
        {project && (
          <span className="flex items-center gap-1.5 text-fg-muted" title={summary?.root ?? ""}>
            <span className="text-fg-faint">◇</span>
            {project}
          </span>
        )}
        {branch && (
          <span className="relative">
            <button className="chip bg-white/[0.04] hover:bg-white/[0.08]" onClick={openHub} title="Branch hub">
              <span className="text-running">⎇</span>
              <span className="text-fg-muted">{branch}</span>
              {hub && (hub.ahead > 0 || hub.behind > 0) && (
                <span className="text-fg-faint">↑{hub.ahead} ↓{hub.behind}</span>
              )}
            </button>
            {hub && (
              <div className="absolute bottom-6 left-0 z-50 w-56 rounded-lg border border-border bg-bg-panel shadow-2xl p-1.5 text-xs">
                <div className="flex items-center justify-between px-1.5 py-1 text-fg-faint text-2xs">
                  <span>↑ {hub.ahead} outgoing · ↓ {hub.behind} incoming</span>
                  <button onClick={openPr} className="text-accent hover:underline">PR ↗</button>
                </div>
                <div className="max-h-48 overflow-auto">
                  {hub.branches.map((b) => (
                    <button
                      key={b.name}
                      onClick={() => switchBranch(b.name)}
                      className={`w-full text-left px-2 py-1 rounded hover:bg-bg-hover ${b.current ? "text-success" : "text-fg-muted"}`}
                    >
                      {b.current ? "✓ " : ""}{b.name}
                    </button>
                  ))}
                  {hub.branches.length === 0 && <div className="px-2 py-1 text-fg-faint">No branches.</div>}
                </div>
              </div>
            )}
          </span>
        )}
        <span className="chip bg-white/[0.04]">
          <span className="dot" style={{ background: indexing ? "#f0b429" : "#3fd07e" }} />
          {indexing ? "indexing" : summary ? "indexed" : "no project"}
        </span>
        {summary && (
          <span className="text-fg-faint hidden md:inline">
            {summary.symbols} symbols · {summary.references} refs · {summary.routes} routes ·{" "}
            {summary.models} models
          </span>
        )}
      </div>
      <div className="flex items-center gap-2">
        {activePath && (
          <span className="truncate max-w-[28vw] text-fg-faint hidden lg:inline">
            {activePath}
            {dirty ? " ●" : ""}
          </span>
        )}
        {stats && (
          <>
            <span className="chip bg-[#8993be]/15 text-[#aab2e0]" title="PHP runtime / project constraint">
              {stats.php_version}
            </span>
            <span
              className="chip bg-white/[0.04] text-fg-muted"
              title="Photon process memory (RAM)"
            >
              <span className="dot" style={{ background: stats.memory_mb > 500 ? "#f0b429" : "#3fd07e" }} />
              {stats.memory_mb} MB
            </span>
          </>
        )}
        <span className="text-fg-faint">UTF-8</span>
        <span
          className="inline-flex items-center justify-center w-[18px] h-[18px] rounded-[5px] bg-accent text-white font-semibold leading-none"
          style={{ fontSize: "10px" }}
          title="Photon IDE 2.16"
        >
          P
        </span>
      </div>
    </div>
  );
}
