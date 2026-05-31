import { useEffect, useMemo } from "react";

export interface MenuItem {
  label?: string;
  /** A keyboard-shortcut hint shown on the right. */
  accel?: string;
  /** When omitted, the row renders as a separator. */
  onClick?: () => void;
  danger?: boolean;
  icon?: string;
  separator?: boolean;
}

// A floating, keyboard-dismissable context menu anchored at (x, y).
export default function ContextMenu({
  x,
  y,
  items,
  onClose,
}: {
  x: number;
  y: number;
  items: MenuItem[];
  onClose: () => void;
}) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const pos = useMemo(() => {
    const w = 260;
    const h = items.length * 30 + 12;
    return {
      left: Math.max(8, Math.min(x, window.innerWidth - w - 8)),
      top: Math.max(8, Math.min(y, window.innerHeight - h - 8)),
      width: w,
    };
  }, [x, y, items.length]);

  return (
    <div className="fixed inset-0 z-[60]" onClick={onClose} onContextMenu={(e) => { e.preventDefault(); onClose(); }}>
      <div
        className="pop-in absolute surface-4 border border-line/60 rounded-lg py-1"
        style={pos}
        onClick={(e) => e.stopPropagation()}
      >
        {items.map((it, i) =>
          it.separator ? (
            <div key={i} className="my-1 border-t border-line/60" />
          ) : (
            <button
              key={i}
              onClick={() => {
                it.onClick?.();
                onClose();
              }}
              className={`w-full flex items-center gap-2.5 px-3 py-1.5 text-sm text-left hover:bg-accent/20 ${
                it.danger ? "text-danger" : "text-fg-muted hover:text-fg"
              }`}
            >
              <span className="w-4 text-center text-fg-faint">{it.icon ?? ""}</span>
              <span className="flex-1">{it.label}</span>
              {it.accel && <span className="kbd">{it.accel}</span>}
            </button>
          )
        )}
      </div>
    </div>
  );
}
