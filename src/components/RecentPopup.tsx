import { useEffect, useState } from "react";

export interface RecentLocation {
  file: string;
  line: number;
}

// Ctrl/Cmd+E — recent files (MRU) and recent navigation locations.
export default function RecentPopup({
  files,
  locations,
  onClose,
  onPick,
}: {
  files: string[];
  locations: RecentLocation[];
  onClose: () => void;
  onPick: (file: string, line: number) => void;
}) {
  const items: RecentLocation[] = [
    ...files.map((f) => ({ file: f, line: 1 })),
    ...locations,
  ];
  const [sel, setSel] = useState(0);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
      else if (e.key === "ArrowDown") { e.preventDefault(); setSel((s) => Math.min(s + 1, items.length - 1)); }
      else if (e.key === "ArrowUp") { e.preventDefault(); setSel((s) => Math.max(s - 1, 0)); }
      else if (e.key === "Enter" && items[sel]) {
        e.preventDefault();
        onPick(items[sel].file, items[sel].line);
        onClose();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [items, sel, onClose, onPick]);

  const base = (p: string) => p.split("/").pop();
  const dir = (p: string) => p.split("/").slice(0, -1).join("/");

  const Row = ({ it, i }: { it: RecentLocation; i: number }) => (
    <div
      onMouseEnter={() => setSel(i)}
      onClick={() => { onPick(it.file, it.line); onClose(); }}
      className={`flex items-center gap-2 px-3 py-1.5 cursor-pointer ${i === sel ? "bg-accent/20" : "hover:bg-white/[0.04]"}`}
    >
      <span className="text-fg truncate">{base(it.file)}</span>
      {it.line > 1 && <span className="text-fg-faint text-xs">:{it.line}</span>}
      <span className="text-fg-faint text-xs truncate flex-1">{dir(it.file)}</span>
    </div>
  );

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center pt-[14vh] bg-black/50 backdrop-blur-sm" onClick={onClose}>
      <div
        className="pop-in w-[560px] max-w-[90vw] surface-5 border border-line/60 rounded-xl overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="max-h-[55vh] overflow-y-auto py-1">
          {files.length > 0 && <div className="panel-title">Recent Files</div>}
          {files.map((f, i) => <Row key={`f-${f}`} it={{ file: f, line: 1 }} i={i} />)}
          {locations.length > 0 && <div className="panel-title">Recent Locations</div>}
          {locations.map((l, i) => <Row key={`l-${i}`} it={l} i={files.length + i} />)}
          {items.length === 0 && <div className="px-3 py-5 text-fg-faint text-sm text-center">No recent items</div>}
        </div>
        <div className="flex items-center gap-3 px-3 py-1.5 border-t border-line text-fg-faint text-2xs">
          <span><span className="kbd">↑↓</span> navigate</span>
          <span><span className="kbd">↵</span> open</span>
          <span><span className="kbd">esc</span> close</span>
        </div>
      </div>
    </div>
  );
}
