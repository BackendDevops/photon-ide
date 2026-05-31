import { useEffect, useState } from "react";
import { api, type GitCommit } from "../lib/api";

type Action = "pick" | "squash" | "fixup" | "drop";
type Row = GitCommit & { action: Action };

const ACTIONS: { value: Action; label: string; hint: string }[] = [
  { value: "pick", label: "pick", hint: "keep this commit" },
  { value: "squash", label: "squash", hint: "merge into previous, combine messages" },
  { value: "fixup", label: "fixup", hint: "merge into previous, drop this message" },
  { value: "drop", label: "drop", hint: "remove this commit" },
];

// Interactive rebase timeline (docs/16 §3): reorder, squash/fixup, or drop the
// commits in base..HEAD, then run the rebase non-interactively.
export default function RebaseModal({
  base,
  onClose,
  onDone,
}: {
  base: string;
  onClose: () => void;
  onDone: (msg: string) => void;
}) {
  const [rows, setRows] = useState<Row[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api
      .gitRebaseList(base)
      .then((cs) => setRows(cs.map((c) => ({ ...c, action: "pick" as Action }))))
      .catch((e) => setError(String(e)));
  }, [base]);

  const move = (i: number, dir: -1 | 1) => {
    setRows((r) => {
      const j = i + dir;
      if (j < 0 || j >= r.length) return r;
      const next = [...r];
      [next[i], next[j]] = [next[j], next[i]];
      return next;
    });
  };
  const setAction = (i: number, action: Action) =>
    setRows((r) => r.map((x, k) => (k === i ? { ...x, action } : x)));

  const firstKept = rows.find((r) => r.action !== "drop");
  const invalid = firstKept ? firstKept.action === "squash" || firstKept.action === "fixup" : false;
  const allDropped = rows.length > 0 && rows.every((r) => r.action === "drop");

  const apply = async () => {
    const todo = rows.map((r) => `${r.action} ${r.hash} ${r.subject}`).join("\n");
    setBusy(true);
    try {
      const msg = await api.gitRebaseInteractive(base, todo);
      onDone(msg || "Rebase complete");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={onClose}>
      <div
        className="w-[640px] max-h-[80vh] flex flex-col rounded-lg border border-border bg-bg-panel shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-4 py-3 border-b border-border flex items-center">
          <span className="text-sm text-fg font-medium">Interactive rebase</span>
          <span className="text-fg-faint text-xs ml-2">
            {rows.length} commit{rows.length === 1 ? "" : "s"} onto {base.slice(0, 7)}
          </span>
          <button onClick={onClose} className="ml-auto text-fg-faint hover:text-fg text-sm">✕</button>
        </div>

        <div className="flex-1 overflow-auto p-2 space-y-1">
          {rows.map((r, i) => (
            <div
              key={r.hash}
              className={`flex items-center gap-2 px-2 py-1.5 rounded border ${
                r.action === "drop" ? "border-line/40 opacity-50" : "border-line"
              } bg-bg-elevated/40`}
            >
              <div className="flex flex-col">
                <button onClick={() => move(i, -1)} disabled={i === 0} className="text-2xs text-fg-faint hover:text-fg disabled:opacity-30 leading-none">▲</button>
                <button onClick={() => move(i, 1)} disabled={i === rows.length - 1} className="text-2xs text-fg-faint hover:text-fg disabled:opacity-30 leading-none">▼</button>
              </div>
              <select
                value={r.action}
                onChange={(e) => setAction(i, e.target.value as Action)}
                className="bg-bg-elevated border border-border rounded px-1 py-0.5 text-xs outline-none"
                title={ACTIONS.find((a) => a.value === r.action)?.hint}
              >
                {ACTIONS.map((a) => (
                  <option key={a.value} value={a.value}>{a.label}</option>
                ))}
              </select>
              <span className="font-mono text-2xs text-fg-faint">{r.short}</span>
              <span className={`truncate flex-1 text-xs ${r.action === "drop" ? "line-through" : "text-fg"}`}>
                {r.subject}
              </span>
            </div>
          ))}
          {rows.length === 0 && !error && (
            <div className="px-2 py-4 text-fg-faint text-xs">No commits to rebase above this point.</div>
          )}
        </div>

        {error && <div className="px-4 py-2 text-danger text-xs border-t border-border">{error}</div>}
        {invalid && (
          <div className="px-4 py-1.5 text-[#d29922] text-2xs border-t border-border">
            The first kept commit can't be squash/fixup — it has nothing to combine into.
          </div>
        )}

        <div className="px-4 py-3 border-t border-border flex items-center gap-2">
          <span className="text-2xs text-fg-faint">Order is top → bottom (oldest → newest). Runs with --autostash; aborts safely on conflict.</span>
          <div className="ml-auto flex gap-2">
            <button onClick={onClose} className="text-xs px-3 py-1 rounded bg-bg-elevated border border-border hover:bg-bg-hover">
              Cancel
            </button>
            <button
              onClick={apply}
              disabled={busy || invalid || allDropped || rows.length === 0}
              className="text-xs px-3 py-1 rounded bg-accent text-white hover:bg-accent-hover disabled:opacity-40"
            >
              Start rebase
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
