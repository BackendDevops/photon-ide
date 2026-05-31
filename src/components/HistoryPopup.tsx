import { useEffect, useState } from "react";
import { api } from "../lib/api";

// Local History (Git-independent): timestamped snapshots taken on every save.
export default function HistoryPopup({
  path,
  onClose,
  onDiff,
  onRestore,
}: {
  path: string;
  onClose: () => void;
  onDiff: (ts: number) => void;
  onRestore: (ts: number) => void;
}) {
  const [items, setItems] = useState<number[] | null>(null);

  useEffect(() => {
    api.historyList(path).then(setItems).catch(() => setItems([]));
  }, [path]);

  const fmt = (ts: number) => {
    const d = new Date(ts);
    return d.toLocaleString(undefined, {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
    });
  };

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/40 pt-24" onClick={onClose}>
      <div
        className="w-[460px] max-h-[60vh] flex flex-col rounded-lg border border-border bg-bg-panel shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-4 py-2.5 border-b border-border flex items-center text-sm">
          <span className="text-fg-muted">Local History · {path.split("/").pop()}</span>
          <button onClick={onClose} className="ml-auto text-fg-faint hover:text-fg">✕</button>
        </div>
        <div className="flex-1 overflow-auto py-1">
          {items === null && <div className="px-4 py-3 text-fg-faint text-xs">Loading…</div>}
          {items?.length === 0 && (
            <div className="px-4 py-3 text-fg-faint text-xs">No snapshots yet — they're captured on save.</div>
          )}
          {items?.map((ts) => (
            <div key={ts} className="flex items-center gap-2 px-3 py-1.5 hover:bg-bg-hover text-xs">
              <span className="flex-1 text-fg">{fmt(ts)}</span>
              <button
                onClick={() => onDiff(ts)}
                className="px-2 py-0.5 rounded bg-bg-elevated border border-border hover:bg-bg-hover text-2xs"
              >
                Diff
              </button>
              <button
                onClick={() => onRestore(ts)}
                className="px-2 py-0.5 rounded bg-accent/20 text-accent border border-accent/40 hover:bg-accent/30 text-2xs"
              >
                Restore
              </button>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
