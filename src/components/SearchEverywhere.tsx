import { useEffect, useMemo, useRef, useState } from "react";
import { api, type SearchHit } from "../lib/api";
import { CategoryTag, KindBadge } from "./icons";

export interface PaletteAction {
  label: string;
  detail?: string;
  category: "action" | "setting";
  run: () => void;
}

// Raycast/JetBrains-style command palette. Merges client-side Actions/Settings
// with streamed backend results (files/symbols/routes).
export default function SearchEverywhere({
  open,
  actions,
  onClose,
  onPick,
}: {
  open: boolean;
  actions: PaletteAction[];
  onClose: () => void;
  onPick: (file: string, line: number) => void;
}) {
  const [query, setQuery] = useState("");
  const [hits, setHits] = useState<SearchHit[]>([]);
  const [sel, setSel] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const reqId = useRef(0);

  useEffect(() => {
    if (open) {
      setQuery("");
      setHits([]);
      setSel(0);
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const id = ++reqId.current;
    if (!query.trim()) {
      setHits([]);
      return;
    }
    const t = setTimeout(async () => {
      try {
        const results = await api.searchEverywhere(query);
        if (id === reqId.current) {
          setHits(results);
          setSel(0);
        }
      } catch {
        /* no project open yet */
      }
    }, 40);
    return () => clearTimeout(t);
  }, [query, open]);

  // Client-side Actions/Settings matches (ranked: prefix > substring).
  const actionMatches = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return [];
    return actions
      .map((a) => {
        const l = a.label.toLowerCase();
        const score = l.startsWith(q) ? 2 : l.includes(q) ? 1 : 0;
        return { a, score };
      })
      .filter((x) => x.score > 0)
      .sort((x, y) => y.score - x.score)
      .slice(0, 6)
      .map((x) => x.a);
  }, [query, actions]);

  if (!open) return null;

  const total = actionMatches.length + hits.length;
  const pickIndex = (i: number) => {
    if (i < actionMatches.length) {
      actionMatches[i].run();
    } else {
      const h = hits[i - actionMatches.length];
      if (h) onPick(h.file, h.line);
    }
    onClose();
  };

  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") onClose();
    else if (e.key === "ArrowDown") { e.preventDefault(); setSel((s) => Math.min(s + 1, total - 1)); }
    else if (e.key === "ArrowUp") { e.preventDefault(); setSel((s) => Math.max(s - 1, 0)); }
    else if (e.key === "Enter" && total > 0) { e.preventDefault(); pickIndex(sel); }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-[12vh] bg-black/50 backdrop-blur-sm"
      onClick={onClose}
    >
      <div
        className="pop-in w-[660px] max-w-[90vw] surface-5 border border-line/60 rounded-xl overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="flex items-center gap-2 px-4 border-b border-line">
          <span className="text-fg-faint text-md">⌕</span>
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={onKey}
            placeholder="Search files, symbols, routes, actions, settings…"
            className="flex-1 bg-transparent py-3.5 text-md outline-none text-fg placeholder:text-fg-faint"
          />
        </div>
        <div className="max-h-[50vh] overflow-y-auto py-1">
          {total === 0 && query.trim() !== "" && (
            <div className="px-4 py-6 text-center text-fg-faint text-sm">No matches</div>
          )}
          {actionMatches.map((a, i) => (
            <div
              key={`a-${a.label}`}
              className={`flex items-center gap-2 px-3 py-1.5 cursor-pointer ${i === sel ? "bg-accent/20" : "hover:bg-white/[0.04]"}`}
              onMouseEnter={() => setSel(i)}
              onClick={() => pickIndex(i)}
            >
              <span className="w-[15px] inline-flex justify-center text-accent">
                {a.category === "setting" ? "⚙" : "›"}
              </span>
              <span className="truncate text-fg">{a.label}</span>
              <span className="truncate text-fg-faint text-xs flex-1">{a.detail ?? ""}</span>
              <CategoryTag category={a.category} />
            </div>
          ))}
          {hits.map((h, i) => {
            const idx = actionMatches.length + i;
            return (
              <div
                key={`${h.category}-${h.file}-${h.line}-${i}`}
                className={`flex items-center gap-2 px-3 py-1.5 cursor-pointer ${idx === sel ? "bg-accent/20" : "hover:bg-white/[0.04]"}`}
                onMouseEnter={() => setSel(idx)}
                onClick={() => pickIndex(idx)}
              >
                {h.category === "symbol" ? (
                  <KindBadge kind={h.kind} />
                ) : (
                  <span className="w-[15px] inline-flex justify-center">{h.category === "route" ? "→" : "▤"}</span>
                )}
                <span className="truncate text-fg">{h.label}</span>
                <span className="truncate text-fg-faint text-xs flex-1">{h.detail}</span>
                <CategoryTag category={h.category} />
              </div>
            );
          })}
        </div>
        <div className="flex items-center gap-3 px-3 py-1.5 border-t border-line text-fg-faint text-2xs">
          <span><span className="kbd">↑↓</span> navigate</span>
          <span><span className="kbd">↵</span> run / open</span>
          <span><span className="kbd">esc</span> close</span>
        </div>
      </div>
    </div>
  );
}
