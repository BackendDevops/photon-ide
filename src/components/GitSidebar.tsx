import { useCallback, useEffect, useState } from "react";
import { api, type Branch, type GitStatus } from "../lib/api";
import ContextMenu, { type MenuItem } from "./ContextMenu";

const STATUS_COLOR: Record<string, string> = {
  modified: "#d29922",
  added: "#3fb950",
  untracked: "#3fb950",
  deleted: "#f85149",
  renamed: "#4c8bf5",
  conflict: "#f85149",
};

// The commit workspace (GitKraken-inspired): staged/unstaged lists, an
// AI-suggested commit message, branch switching, and remote actions.
// (docs/16-git-experience.md — commit workspace)
export default function GitSidebar({
  onChanged,
  onOpenDiff,
  onOpenFile,
  onResolveConflict,
  onToast,
}: {
  onChanged: () => void;
  onOpenDiff: (file: string) => void;
  onOpenFile: (file: string) => void;
  onResolveConflict?: (file: string) => void;
  onToast?: (msg: string) => void;
}) {
  const [menu, setMenu] = useState<{ x: number; y: number; file: string; staged: boolean } | null>(null);
  const [status, setStatus] = useState<GitStatus | null>(null);
  const [branches, setBranches] = useState<Branch[]>([]);
  const [conflicts, setConflicts] = useState<string[]>([]);
  const [message, setMessage] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [amend, setAmend] = useState(false);
  // Inline per-hunk staging: which file is expanded + its parsed hunks.
  const [hunkOf, setHunkOf] = useState<{ path: string; staged: boolean } | null>(null);
  const [hunks, setHunks] = useState<{ header: string; body: string; text: string }[]>([]);

  const refresh = useCallback(async () => {
    try {
      const [s, b, c] = await Promise.all([
        api.gitStatus(),
        api.gitBranches(),
        api.gitConflicts().catch(() => [] as string[]),
      ]);
      setStatus(s);
      setBranches(b);
      setConflicts(c);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const act = async (fn: () => Promise<unknown>, label?: string) => {
    setBusy(true);
    try {
      const out = await fn();
      await refresh();
      onChanged();
      if (label) {
        const detail =
          typeof out === "string" && out.trim()
            ? out.trim().split("\n").filter(Boolean).slice(-1)[0].slice(0, 80)
            : "";
        onToast?.(detail ? `${label} — ${detail}` : label);
      }
    } catch (e) {
      const msg = String(e).split("\n")[0].slice(0, 100);
      setError(String(e));
      onToast?.(`${label ?? "Git"} failed: ${msg}`);
    } finally {
      setBusy(false);
    }
  };

  const suggest = async () => {
    try {
      const m = await api.gitSuggestMessage();
      if (m) setMessage(m);
    } catch (e) {
      setError(String(e));
    }
  };

  // Split a single-file unified diff into a shared header + individual hunks.
  // Each hunk's `text` is a self-contained patch (header + that @@ block).
  const parseHunks = (diff: string) => {
    const lines = diff.split("\n");
    const firstHunk = lines.findIndex((l) => l.startsWith("@@"));
    if (firstHunk < 0) return { header: "", list: [] as { header: string; body: string; text: string }[] };
    const header = lines.slice(0, firstHunk).join("\n");
    const list: { header: string; body: string; text: string }[] = [];
    let cur: string[] = [];
    for (const l of lines.slice(firstHunk)) {
      if (l.startsWith("@@") && cur.length) {
        const body = cur.join("\n");
        list.push({ header: cur[0], body, text: `${header}\n${body}\n` });
        cur = [];
      }
      cur.push(l);
    }
    if (cur.length) {
      const body = cur.join("\n");
      list.push({ header: cur[0], body, text: `${header}\n${body}\n` });
    }
    return { header, list };
  };

  const toggleHunks = async (path: string, staged: boolean) => {
    if (hunkOf && hunkOf.path === path && hunkOf.staged === staged) {
      setHunkOf(null);
      setHunks([]);
      return;
    }
    try {
      const diff = await api.gitFileDiff(path, staged);
      setHunks(parseHunks(diff).list);
      setHunkOf({ path, staged });
    } catch (e) {
      setError(String(e));
    }
  };

  const applyHunk = (text: string, reverse: boolean) =>
    act(async () => {
      await api.gitApplyHunk(text, reverse);
      if (hunkOf) {
        // refresh the open hunk list against the new index state
        const diff = await api.gitFileDiff(hunkOf.path, hunkOf.staged);
        setHunks(parseHunks(diff).list);
      }
    });

  if (error && !status) {
    return (
      <div className="px-3 py-4 text-fg-faint text-xs">
        Not a git repository, or git is unavailable.
        <div className="text-danger mt-1">{error}</div>
      </div>
    );
  }

  const staged = status?.files.filter((f) => f.staged) ?? [];
  const unstaged = status?.files.filter((f) => !f.staged) ?? [];

  const FileRow = ({
    path,
    label,
    action,
    sign,
  }: {
    path: string;
    label: string;
    action: () => void;
    sign: string;
  }) => (
    <div
      className="row group"
      onClick={() => onOpenDiff(path)}
      onContextMenu={(e) => {
        e.preventDefault();
        setMenu({ x: e.clientX, y: e.clientY, file: path, staged: sign === "−" });
      }}
      title={path}
    >
      <span
        className="w-3 text-center text-xs font-bold"
        style={{ color: STATUS_COLOR[label] || "#8b949e" }}
      >
        {label[0]?.toUpperCase()}
      </span>
      <span className="truncate flex-1 text-xs">
        {path.split("/").pop()}
      </span>
      <button
        className="opacity-0 group-hover:opacity-100 text-fg-faint hover:text-fg text-2xs px-1"
        onClick={(e) => {
          e.stopPropagation();
          void toggleHunks(path, sign === "−");
        }}
        title="Stage / unstage individual hunks"
      >
        ❏
      </button>
      <button
        className="opacity-0 group-hover:opacity-100 text-fg-faint hover:text-fg text-xs px-1"
        onClick={(e) => {
          e.stopPropagation();
          action();
        }}
        title={sign === "+" ? "Stage" : "Unstage"}
      >
        {sign}
      </button>
    </div>
  );

  // Inline hunk list for the currently-expanded file.
  const HunkPanel = ({ path, staged }: { path: string; staged: boolean }) => {
    if (!hunkOf || hunkOf.path !== path || hunkOf.staged !== staged) return null;
    if (hunks.length === 0)
      return <div className="px-6 py-1.5 text-2xs text-fg-faint">No hunks (binary or whole-file change).</div>;
    return (
      <div className="pl-5 pr-2 pb-1.5 space-y-1.5 bg-bg-elevated/40">
        {hunks.map((h, i) => (
          <div key={i} className="rounded border border-line/60 overflow-hidden">
            <pre className="text-[10px] leading-tight font-mono max-h-28 overflow-auto m-0 p-1.5">
              {h.body.split("\n").map((l, j) => (
                <div
                  key={j}
                  style={{
                    color: l.startsWith("+")
                      ? "#3fb950"
                      : l.startsWith("-")
                        ? "#f85149"
                        : l.startsWith("@@")
                          ? "#9d6cff"
                          : "var(--fg-faint)",
                  }}
                >
                  {l || " "}
                </div>
              ))}
            </pre>
            <div className="flex justify-end px-1.5 py-1 border-t border-line/60">
              <button
                disabled={busy}
                onClick={() => applyHunk(h.text, staged)}
                className="text-2xs px-2 py-0.5 rounded bg-surface-3 border border-line hover:bg-bg-hover"
                title={staged ? "Remove this hunk from the index" : "Add this hunk to the index"}
              >
                {staged ? "− Unstage hunk" : "+ Stage hunk"}
              </button>
            </div>
          </div>
        ))}
      </div>
    );
  };

  return (
    <div className="h-full flex flex-col text-sm">
      {/* branch + remote actions */}
      <div className="px-2 py-2 border-b border-border space-y-2">
        <div className="flex items-center gap-2">
          <span className="text-[#9d6cff]">⎇</span>
          <select
            value={status?.branch ?? ""}
            onChange={(e) => act(() => api.gitCheckout(e.target.value))}
            className="flex-1 bg-bg-elevated border border-border rounded px-1.5 py-1 text-xs outline-none"
          >
            {branches.map((b) => (
              <option key={b.name} value={b.name}>
                {b.name}
              </option>
            ))}
            {status && !branches.length && (
              <option>{status.branch}</option>
            )}
          </select>
        </div>
        <div className="flex items-center gap-1.5 text-xs">
          {status && (status.ahead > 0 || status.behind > 0) && (
            <span className="text-fg-faint">
              ↑{status.ahead} ↓{status.behind}
            </span>
          )}
          <button
            onClick={() => act(api.gitPull, "Pulled")}
            disabled={busy}
            className="px-2 py-0.5 rounded bg-bg-elevated border border-border hover:bg-bg-hover"
          >
            Pull
          </button>
          <button
            onClick={() => act(api.gitPush, "Pushed")}
            disabled={busy}
            className="px-2 py-0.5 rounded bg-bg-elevated border border-border hover:bg-bg-hover"
          >
            Push
          </button>
          <button
            onClick={() => act(api.gitStash, "Stashed")}
            disabled={busy}
            className="px-2 py-0.5 rounded bg-bg-elevated border border-border hover:bg-bg-hover"
          >
            Stash
          </button>
          <button
            onClick={async () => {
              try {
                const url = await api.gitPrUrl();
                await api.openExternal(url);
              } catch (e) {
                setError(String(e));
              }
            }}
            disabled={busy}
            title="Open a pull/merge request for the current branch in your browser"
            className="px-2 py-0.5 rounded bg-bg-elevated border border-border hover:bg-bg-hover ml-auto"
          >
            PR ↗
          </button>
        </div>
      </div>

      {/* changes */}
      <div className="flex-1 overflow-y-auto">
        {conflicts.length > 0 && (
          <>
            <div className="panel-title text-danger">Conflicts ({conflicts.length})</div>
            {conflicts.map((f) => (
              <div key={f} className="px-3 py-1.5 border-b border-line/40">
                <div className="flex items-center gap-1.5">
                  <span className="text-danger">⚠</span>
                  <span className="text-fg truncate flex-1 text-xs" title={f}>
                    {f.split("/").pop()}
                  </span>
                </div>
                <div className="flex flex-wrap gap-1.5 mt-1 pl-5">
                  <button
                    onClick={() => onResolveConflict?.(f)}
                    className="text-2xs px-2 py-0.5 rounded bg-accent/20 text-accent border border-accent/40 hover:bg-accent/30"
                    title="Open the 3-way conflict resolution center"
                  >
                    Resolve…
                  </button>
                  <button
                    onClick={() => act(() => api.gitResolve(f, "ours"))}
                    className="text-2xs px-2 py-0.5 rounded bg-surface-3 border border-line hover:bg-bg-hover"
                  >
                    Use ours
                  </button>
                  <button
                    onClick={() => act(() => api.gitResolve(f, "theirs"))}
                    className="text-2xs px-2 py-0.5 rounded bg-surface-3 border border-line hover:bg-bg-hover"
                  >
                    Use theirs
                  </button>
                </div>
              </div>
            ))}
          </>
        )}
        {status?.clean && conflicts.length === 0 && (
          <div className="px-3 py-4 text-fg-faint text-xs">
            Working tree clean. ✓
          </div>
        )}
        {staged.length > 0 && (
          <>
            <div className="flex items-center justify-between panel-title">
              <span>Staged ({staged.length})</span>
              <button
                className="text-fg-faint hover:text-fg lowercase tracking-normal"
                onClick={() =>
                  act(() => api.gitUnstage(staged.map((f) => f.path)))
                }
              >
                unstage all
              </button>
            </div>
            {staged.map((f) => (
              <div key={f.path}>
                <FileRow
                  path={f.path}
                  label={f.label}
                  sign="−"
                  action={() => act(() => api.gitUnstage([f.path]))}
                />
                <HunkPanel path={f.path} staged={true} />
              </div>
            ))}
          </>
        )}
        {unstaged.length > 0 && (
          <>
            <div className="flex items-center justify-between panel-title">
              <span>Changes ({unstaged.length})</span>
              <button
                className="text-fg-faint hover:text-fg lowercase tracking-normal"
                onClick={() =>
                  act(() => api.gitStage(unstaged.map((f) => f.path)))
                }
              >
                stage all
              </button>
            </div>
            {unstaged.map((f) => (
              <div key={f.path}>
                <FileRow
                  path={f.path}
                  label={f.label}
                  sign="+"
                  action={() => act(() => api.gitStage([f.path]))}
                />
                <HunkPanel path={f.path} staged={false} />
              </div>
            ))}
          </>
        )}
      </div>

      {/* commit box */}
      <div className="border-t border-border p-2 space-y-2">
        <div className="relative">
          <textarea
            value={message}
            onChange={(e) => setMessage(e.target.value)}
            placeholder="Commit message…"
            className="w-full h-16 bg-bg-elevated border border-border rounded px-2 py-1.5 text-xs outline-none focus:border-accent resize-none"
          />
          <button
            onClick={suggest}
            title="Suggest a commit message from the staged changes"
            className="absolute bottom-1.5 right-1.5 text-[10px] px-1.5 py-0.5 rounded bg-accent/20 text-accent hover:bg-accent/30"
          >
            ✨ Suggest
          </button>
        </div>
        <label className="flex items-center gap-1.5 text-2xs text-fg-faint cursor-pointer select-none">
          <input
            type="checkbox"
            checked={amend}
            onChange={(e) => setAmend(e.target.checked)}
            className="accent-accent"
          />
          Amend last commit
        </label>
        <button
          onClick={() =>
            act(async () => {
              const r = amend ? await api.gitAmend(message) : await api.gitCommit(message);
              setMessage("");
              setAmend(false);
              return r;
            }, amend ? "Amended" : "Committed")
          }
          disabled={
            busy ||
            (amend ? !message.trim() && staged.length === 0 : !message.trim() || staged.length === 0)
          }
          className="w-full text-xs py-1.5 rounded bg-accent text-white hover:bg-accent-hover disabled:opacity-40"
        >
          {amend ? "Amend" : "Commit"} {staged.length > 0 ? `(${staged.length})` : ""}
        </button>
      </div>

      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(null)}
          items={
            [
              { label: "Show Diff", icon: "⇄", accel: "⌘D", onClick: () => onOpenDiff(menu.file) },
              { label: "Jump to Source", icon: "↪", onClick: () => onOpenFile(menu.file) },
              { separator: true },
              menu.staged
                ? { label: "Unstage", icon: "−", onClick: () => act(() => api.gitUnstage([menu.file])) }
                : { label: "Stage", icon: "+", onClick: () => act(() => api.gitStage([menu.file])) },
              { label: "Stage & Commit…", icon: "◦", onClick: () => act(() => api.gitStage([menu.file])) },
              { separator: true },
              {
                label: "Rollback / Discard…",
                icon: "↺",
                accel: "⌥⌘Z",
                danger: true,
                onClick: () => act(() => api.gitDiscard(menu.file)),
              },
            ] as MenuItem[]
          }
        />
      )}
    </div>
  );
}
