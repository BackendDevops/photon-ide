import { useEffect, useRef, useState } from "react";
import { api, type ChangeSet } from "../lib/api";

// JetBrains-style inline Safe Rename: a compact, non-modal box anchored over the
// editor. Type → live preview of every edit (uncertain ones flagged + toggleable)
// → apply atomically. No full-screen backdrop, so context stays visible.
export default function InlineRename({
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
  const [showPreview, setShowPreview] = useState(false);
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
      setShowPreview(true);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const apply = async () => {
    let cs = plan;
    if (!cs) {
      // Enter with no preview yet → plan inline, then apply.
      try {
        setBusy(true);
        cs = await api.planRename(oldName, newName);
        setPlan(cs);
      } catch (e) {
        setError(String(e));
        setBusy(false);
        return;
      }
    }
    setBusy(true);
    try {
      const accepted = cs.edits.map((_, i) => i).filter((i) => !excluded.has(i));
      const n = await api.applyRename(cs, accepted);
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
    <div className="absolute top-3 left-1/2 -translate-x-1/2 z-40 w-[440px] max-w-[92%] pop-in">
      <div className="bg-bg-panel/95 backdrop-blur border border-accent/60 rounded-lg shadow-2xl ring-1 ring-accent/20 overflow-hidden">
        <div className="flex items-center gap-2 px-2.5 py-2">
          <span className="text-2xs uppercase tracking-wider text-fg-faint shrink-0">Rename</span>
          <input
            ref={inputRef}
            value={newName}
            onChange={(e) => {
              setNewName(e.target.value);
              setPlan(null);
              setShowPreview(false);
            }}
            onKeyDown={(e) => {
              if (e.key === "Enter") void apply();
              if (e.key === "Escape") onClose();
            }}
            spellCheck={false}
            className="flex-1 bg-bg-elevated border border-border rounded px-2 py-1 text-sm font-mono text-fg outline-none focus:border-accent"
          />
          <button
            onClick={preview}
            disabled={busy || !newName || newName === oldName}
            title="Preview affected edits"
            className="text-2xs px-2 py-1 rounded text-fg-muted hover:bg-bg-hover disabled:opacity-40 shrink-0"
          >
            Preview
          </button>
          <button
            onClick={() => void apply()}
            disabled={busy || !newName || newName === oldName}
            className="text-2xs px-2.5 py-1 rounded bg-accent text-white hover:bg-accent-hover disabled:opacity-40 shrink-0"
          >
            Rename
          </button>
        </div>

        {error && <div className="px-3 pb-2 text-danger text-2xs">{error}</div>}

        {plan && (
          <div className="px-2.5 pb-1 -mt-0.5 flex items-center justify-between text-2xs text-fg-faint">
            <span>
              {plan.edits.length} edit(s) · {plan.files_affected} file(s)
            </span>
            <button
              onClick={() => setShowPreview((v) => !v)}
              className="hover:text-fg"
            >
              {showPreview ? "hide" : "show"} preview
            </button>
          </div>
        )}

        {plan && showPreview && (
          <div className="max-h-52 overflow-y-auto border-t border-line py-1">
            {plan.edits.map((e, i) => (
              <label
                key={i}
                className="flex items-start gap-2 px-2.5 py-1 hover:bg-bg-hover cursor-pointer"
              >
                <input
                  type="checkbox"
                  checked={!excluded.has(i)}
                  onChange={() => toggle(i)}
                  className="mt-0.5"
                />
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span className="text-fg-faint text-2xs truncate">
                      {e.file}:{e.line}
                    </span>
                    {!e.certain && (
                      <span className="text-warn text-[9px] uppercase">uncertain</span>
                    )}
                  </div>
                  <code className="text-2xs text-fg-muted truncate block">{e.preview}</code>
                </div>
              </label>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
