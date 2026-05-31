import { useEffect, useMemo, useRef, useState } from "react";
import type { UsagesResult } from "../lib/api";

const KIND_COLOR: Record<string, string> = {
  type_ref: "#5b8cff",
  static_ref: "#b07bff",
  call: "#3fd07e",
  import: "#9aa4b2",
  member: "#36d6c3",
};

// JetBrains "Show Usages"-style floating popup, anchored near the click.
export default function UsagesPopup({
  result,
  x,
  y,
  onClose,
  onPick,
}: {
  result: UsagesResult;
  x: number;
  y: number;
  onClose: () => void;
  onPick: (file: string, line: number) => void;
}) {
  const [sel, setSel] = useState(0);
  const ref = useRef<HTMLDivElement>(null);

  // Keep the popup on-screen.
  const pos = useMemo(() => {
    const w = 620;
    const left = Math.min(x, window.innerWidth - w - 16);
    const top = Math.min(y + 8, window.innerHeight - 320);
    return { left: Math.max(8, left), top: Math.max(8, top), width: w };
  }, [x, y]);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
      else if (e.key === "ArrowDown") { e.preventDefault(); setSel((s) => Math.min(s + 1, result.hits.length - 1)); }
      else if (e.key === "ArrowUp") { e.preventDefault(); setSel((s) => Math.max(s - 1, 0)); }
      else if (e.key === "Enter" && result.hits[sel]) {
        e.preventDefault();
        onPick(result.hits[sel].file, result.hits[sel].line);
        onClose();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [result, sel, onClose, onPick]);

  return (
    <div className="fixed inset-0 z-50" onClick={onClose}>
      <div
        ref={ref}
        className="pop-in absolute surface-5 border border-line/60 rounded-lg overflow-hidden"
        style={pos}
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between px-3 py-2 border-b border-line bg-white/[0.03]">
          <span className="text-sm text-fg truncate">{result.title}</span>
          <span className="text-xs text-fg-faint shrink-0 ml-2">
            {Math.min(result.hits.length, result.total)} of {result.total} usages
          </span>
        </div>
        <div className="max-h-[280px] overflow-y-auto py-1">
          {result.hits.length === 0 && (
            <div className="px-3 py-4 text-fg-faint text-sm">No usages found.</div>
          )}
          {result.hits.map((h, i) => (
            <div
              key={`${h.file}-${h.line}-${i}`}
              onMouseEnter={() => setSel(i)}
              onClick={() => { onPick(h.file, h.line); onClose(); }}
              className={`flex items-center gap-2 px-3 py-1 cursor-pointer ${
                i === sel ? "bg-accent/20" : "hover:bg-white/[0.04]"
              }`}
            >
              <span className="w-1.5 h-1.5 rounded-full shrink-0" style={{ background: KIND_COLOR[h.kind] || "#9aa4b2" }} />
              <span className="text-fg-faint text-xs shrink-0 w-44 truncate">
                {h.file.split("/").pop()}
                <span className="text-fg-faint/60"> :{h.line}</span>
              </span>
              <code className="text-sm text-fg-muted truncate flex-1">{h.preview}</code>
            </div>
          ))}
          {result.total > result.hits.length && (
            <div className="px-3 py-1.5 text-fg-faint text-xs">
              {result.total - result.hits.length} more usages…
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
