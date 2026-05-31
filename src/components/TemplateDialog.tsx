import { useEffect, useMemo, useState } from "react";
import { api, type Template } from "../lib/api";

// "New from Template": pick a template, fill its fields, preview the output
// path, and create the file.
export default function TemplateDialog({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: (path: string) => void;
}) {
  const [templates, setTemplates] = useState<Template[]>([]);
  const [selected, setSelected] = useState<Template | null>(null);
  const [vars, setVars] = useState<Record<string, string>>({});
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    api
      .templateList()
      .then((t) => {
        setTemplates(t);
        if (t.length) {
          setSelected(t[0]);
        }
      })
      .catch((e) => setError(String(e)));
  }, []);

  useEffect(() => {
    if (selected) {
      const init: Record<string, string> = {};
      selected.fields.forEach((f) => (init[f.key] = f.default));
      setVars(init);
    }
  }, [selected]);

  const grouped = useMemo(() => {
    const m = new Map<string, Template[]>();
    for (const t of templates) {
      if (!m.has(t.category)) m.set(t.category, []);
      m.get(t.category)!.push(t);
    }
    return [...m.entries()];
  }, [templates]);

  const preview = useMemo(() => {
    if (!selected) return "";
    let p = selected.filename;
    for (const [k, v] of Object.entries(vars)) {
      p = p.replaceAll(`{{${k}}}`, v).replaceAll(`{{ ${k} }}`, v);
    }
    return p;
  }, [selected, vars]);

  const create = async () => {
    if (!selected) return;
    setBusy(true);
    setError(null);
    try {
      const path = await api.templateCreate(selected.id, vars);
      onCreated(path);
      onClose();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      onClick={onClose}
    >
      <div
        className="pop-in w-[680px] max-w-[94vw] h-[460px] bg-bg-panel border border-border rounded-xl shadow-2xl flex flex-col"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-4 py-3 border-b border-border text-sm text-fg-muted">
          New from Template
        </div>
        <div className="flex-1 flex min-h-0">
          {/* template list */}
          <div className="w-56 border-r border-border overflow-y-auto py-1">
            {grouped.map(([cat, items]) => (
              <div key={cat}>
                <div className="panel-title">{cat}</div>
                {items.map((t) => (
                  <div
                    key={t.id}
                    onClick={() => setSelected(t)}
                    className={`row ${selected?.id === t.id ? "row-active" : ""}`}
                    title={t.source}
                  >
                    <span className="truncate">{t.label}</span>
                    {t.source.startsWith("ext:") && (
                      <span className="text-[10px] text-accent ml-auto">ext</span>
                    )}
                  </div>
                ))}
              </div>
            ))}
          </div>

          {/* fields */}
          <div className="flex-1 p-4 overflow-y-auto">
            {selected ? (
              <>
                <div className="text-fg mb-3">{selected.label}</div>
                {selected.fields.map((f) => (
                  <label key={f.key} className="block mb-3">
                    <span className="text-fg-muted text-sm block mb-1">{f.label}</span>
                    <input
                      autoFocus={f.key === "name"}
                      value={vars[f.key] ?? ""}
                      onChange={(e) => setVars((v) => ({ ...v, [f.key]: e.target.value }))}
                      onKeyDown={(e) => e.key === "Enter" && create()}
                      className="w-full bg-bg-elevated border border-border rounded px-2 py-1.5 text-sm outline-none focus:border-accent"
                    />
                  </label>
                ))}
                <div className="text-fg-faint text-xs mt-4">
                  Creates: <code className="text-fg-muted">{preview}</code>
                </div>
                {error && <div className="text-danger text-xs mt-2">{error}</div>}
              </>
            ) : (
              <div className="text-fg-faint text-sm">No templates available.</div>
            )}
          </div>
        </div>

        <div className="flex items-center justify-end gap-2 px-4 py-3 border-t border-border">
          <button onClick={onClose} className="text-sm px-3 py-1.5 rounded text-fg-muted hover:bg-bg-hover">
            Cancel
          </button>
          <button
            onClick={create}
            disabled={busy || !selected}
            className="text-sm px-3 py-1.5 rounded bg-accent text-white hover:bg-accent-hover disabled:opacity-40"
          >
            Create
          </button>
        </div>
      </div>
    </div>
  );
}
