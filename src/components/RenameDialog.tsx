import { useEffect, useRef, useState } from "react";
import { api, type ChangeSet } from "../lib/api";

// Plan-then-apply Safe Rename: type a new name, preview every edit (uncertain
// ones flagged + toggleable), then apply atomically. (docs/02 §refactoring)
export default function RenameDialog({
  oldName,
  onClose,
  onApplied,
}: {
  oldName: string;
  onClose: () => void;
  onApplied: (files: number) => void;
}) {
  const [newName, setNewName] = useState(oldName);
  const [plan, setPlan] = useState<ChangeSet | null>(null);
  const [excluded, setExcluded] = useState<Set<number>>(new Set());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    requestAnimationFrame(() => {
      inputRef.current?.focus();
      inputRef.current?.select();
    });
  }, []);

  const preview = async () => {
    setError(null);
    setBusy(true);
    try {
      const cs = await api.planRename(oldName, newName);
      setPlan(cs);
      setExcluded(new Set());
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const apply = async () => {
    if (!plan) return;
    setBusy(true);
    try {
      const accepted = plan.edits
        .map((_, i) => i)
        .filter((i) => !excluded.has(i));
      const n = await api.applyRename(plan, accepted);
      onApplied(n);
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const toggle = (i: number) =>
    setExcluded((s) => {
      const n = new Set(s);
      n.has(i) ? n.delete(i) : n.add(i);
      return n;
    });

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-[12vh] bg-black/40"
      onClick={onClose}
    >
      <div
        className="pop-in w-[680px] max-w-[92vw] bg-bg-panel border border-border rounded-xl shadow-2xl overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-4 py-3 border-b border-border flex items-center gap-3">
          <span className="text-fg-muted text-sm">Rename</span>
          <code className="text-accent">{oldName}</code>
          <span className="text-fg-faint">→</span>
          <input
            ref={inputRef}
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") (plan ? apply() : preview());
              if (e.key === "Escape") onClose();
            }}
            className="flex-1 bg-bg-elevated border border-border rounded px-2 py-1 text-fg outline-none focus:border-accent"
          />
          <button
            onClick={preview}
            disabled={busy || !newName || newName === oldName}
            className="text-xs px-2.5 py-1 rounded bg-bg-elevated border border-border hover:bg-bg-hover disabled:opacity-40"
          >
            Preview
          </button>
        </div>

        {error && (
          <div className="px-4 py-2 text-danger text-xs border-b border-border">
            {error}
          </div>
        )}

        {plan && (
          <>
            <div className="px-4 py-2 text-fg-faint text-xs border-b border-border">
              {plan.edits.length} edit(s) across {plan.files_affected} file(s).
              Uncheck any you don't want.
            </div>
            <div className="max-h-[40vh] overflow-y-auto py-1">
              {plan.edits.map((e, i) => (
                <label
                  key={i}
                  className="flex items-start gap-2 px-3 py-1.5 hover:bg-bg-hover cursor-pointer"
                >
                  <input
                    type="checkbox"
                    checked={!excluded.has(i)}
                    onChange={() => toggle(i)}
                    className="mt-0.5"
                  />
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <span className="text-fg-faint text-xs truncate">
                        {e.file}:{e.line}
                      </span>
                      {!e.certain && (
                        <span className="text-warn text-[10px] uppercase">
                          uncertain
                        </span>
                      )}
                    </div>
                    <code className="text-xs text-fg-muted truncate block">
                      {e.preview}
                    </code>
                  </div>
                </label>
              ))}
            </div>
          </>
        )}

        <div className="flex items-center justify-end gap-2 px-4 py-2.5 border-t border-border">
          <button
            onClick={onClose}
            className="text-xs px-3 py-1.5 rounded text-fg-muted hover:bg-bg-hover"
          >
            Cancel
          </button>
          <button
            onClick={apply}
            disabled={!plan || busy}
            className="text-xs px-3 py-1.5 rounded bg-accent text-white hover:bg-accent-hover disabled:opacity-40"
          >
            Apply Rename
          </button>
        </div>
      </div>
    </div>
  );
}
