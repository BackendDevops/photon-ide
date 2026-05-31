import { useMemo } from "react";
import type { Reference } from "../lib/api";

const KIND_LABEL: Record<string, string> = {
  type_ref: "type",
  static_ref: "static",
  call: "call",
  import: "import",
  member: "member",
};

// Find Usages results, grouped by file. (docs/02 §navigation)
export default function UsagesPanel({
  name,
  usages,
  onClose,
  onPick,
}: {
  name: string;
  usages: Reference[];
  onClose: () => void;
  onPick: (file: string, line: number) => void;
}) {
  const grouped = useMemo(() => {
    const m = new Map<string, Reference[]>();
    for (const u of usages) {
      if (!m.has(u.file)) m.set(u.file, []);
      m.get(u.file)!.push(u);
    }
    return [...m.entries()];
  }, [usages]);

  return (
    <div className="h-full flex flex-col">
      <div className="flex items-center justify-between px-3 py-1.5 border-b border-border">
        <span className="text-xs text-fg-muted">
          Usages of <code className="text-accent">{name}</code> ·{" "}
          {usages.length} in {grouped.length} file(s)
        </span>
        <button onClick={onClose} className="text-fg-faint hover:text-fg text-xs">
          ✕
        </button>
      </div>
      <div className="flex-1 overflow-y-auto py-1 text-sm">
        {grouped.length === 0 && (
          <div className="px-3 py-4 text-fg-faint text-xs">No usages found.</div>
        )}
        {grouped.map(([file, refs]) => (
          <div key={file}>
            <div className="px-3 py-1 text-fg-faint text-xs sticky top-0 bg-bg-panel">
              {file}
            </div>
            {refs.map((r, i) => (
              <div
                key={i}
                className="row pl-6"
                onClick={() => onPick(r.file, r.line)}
              >
                <span className="text-fg-faint text-xs w-10">:{r.line}</span>
                <span className="text-[10px] uppercase text-accent/80 w-14">
                  {KIND_LABEL[r.kind] || r.kind}
                </span>
                <span className="text-fg-muted truncate">{r.name}</span>
              </div>
            ))}
          </div>
        ))}
      </div>
    </div>
  );
}
