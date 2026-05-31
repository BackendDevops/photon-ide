import type { SymbolKind } from "../lib/api";

// A compact letter-badge per symbol kind (JetBrains-style gutter glyphs),
// kept dependency-free.
const KIND_STYLE: Record<SymbolKind, { letter: string; color: string }> = {
  class: { letter: "C", color: "#e5a13a" },
  interface: { letter: "I", color: "#4c8bf5" },
  trait: { letter: "T", color: "#9d6cff" },
  enum: { letter: "E", color: "#3fb950" },
  enum_case: { letter: "e", color: "#3fb950" },
  function: { letter: "ƒ", color: "#c678dd" },
  method: { letter: "m", color: "#c678dd" },
  property: { letter: "p", color: "#56b6c2" },
  constant: { letter: "k", color: "#d29922" },
  namespace: { letter: "N", color: "#8b949e" },
};

export function KindBadge({ kind }: { kind: SymbolKind | string | null }) {
  const style =
    (kind && KIND_STYLE[kind as SymbolKind]) || {
      letter: "•",
      color: "#8b949e",
    };
  return (
    <span
      className="inline-flex items-center justify-center w-[15px] h-[15px] rounded text-[10px] font-bold shrink-0"
      style={{ color: style.color, background: `${style.color}22` }}
    >
      {style.letter}
    </span>
  );
}

const LANG_COLOR: Record<string, string> = {
  php: "#8993be",
  blade: "#f55247",
  js: "#f1e05a",
  ts: "#4c8bf5",
  tsx: "#4c8bf5",
  jsx: "#f1e05a",
  vue: "#41b883",
  json: "#d29922",
  sql: "#56b6c2",
  markdown: "#8b949e",
  css: "#c678dd",
  html: "#e34c26",
  yaml: "#cb171e",
  env: "#3fb950",
};

export function FileGlyph({ lang }: { lang: string }) {
  const color = LANG_COLOR[lang] || "#6e7681";
  return (
    <span
      className="inline-block w-2 h-2 rounded-sm shrink-0"
      style={{ background: color }}
    />
  );
}

// The canonical PHP "elePHPant" logo mark — a violet oval with italic "php".
function PhpLogo() {
  return (
    <svg width="17" height="11" viewBox="0 0 22 13" className="shrink-0" aria-hidden="true">
      <ellipse cx="11" cy="6.5" rx="10.5" ry="6" fill="#777bb3" />
      <text
        x="11"
        y="9.2"
        textAnchor="middle"
        fontFamily="Georgia, 'Times New Roman', serif"
        fontStyle="italic"
        fontWeight="bold"
        fontSize="7"
        fill="#1b1b2b"
      >
        php
      </text>
    </svg>
  );
}

// A tinted document glyph for non-PHP files.
function DocIcon({ color }: { color: string }) {
  return (
    <svg width="14" height="14" viewBox="0 0 16 16" className="shrink-0" aria-hidden="true">
      <path
        d="M4 1.5h4.5L12 5v9a.6.6 0 0 1-.6.6H4a.6.6 0 0 1-.6-.6V2.1A.6.6 0 0 1 4 1.5z"
        fill={`${color}26`}
        stroke={color}
        strokeWidth="1"
        strokeLinejoin="round"
      />
      <path d="M8.4 1.7v3.1h3" fill="none" stroke={color} strokeWidth="1" strokeLinejoin="round" />
    </svg>
  );
}

export function FileIcon({ lang, name }: { lang?: string; name?: string }) {
  const isPhp = lang === "php" || (name ? name.endsWith(".php") : false);
  if (isPhp) return <PhpLogo />;
  return <DocIcon color={LANG_COLOR[lang || "other"] || "#6e7681"} />;
}

// A colored folder (open/closed). The project root renders as a package box.
export function FolderIcon({ open, root }: { open?: boolean; root?: boolean }) {
  if (root) {
    return (
      <svg width="15" height="15" viewBox="0 0 16 16" className="shrink-0" aria-hidden="true">
        <path
          d="M8 1.5l5.5 3v6.9L8 14.5 2.5 11.4V4.5L8 1.5z"
          fill="#e0a45826"
          stroke="#e0a458"
          strokeWidth="1"
          strokeLinejoin="round"
        />
        <path d="M2.7 4.6L8 7.5l5.3-2.9M8 7.5v6.8" fill="none" stroke="#e0a458" strokeWidth="0.9" />
      </svg>
    );
  }
  const c = "#7aa6d6";
  return (
    <svg width="15" height="15" viewBox="0 0 16 16" className="shrink-0" aria-hidden="true">
      <path
        d={
          open
            ? "M1.6 4.4a1 1 0 0 1 1-1h2.7l1.4 1.4h6.7a1 1 0 0 1 1 1v.5h-10l-1.8 6.1V4.4z"
            : "M1.6 4.4a1 1 0 0 1 1-1h2.7l1.4 1.4h6.7a1 1 0 0 1 1 1v5.8a1 1 0 0 1-1 1H2.6a1 1 0 0 1-1-1V4.4z"
        }
        fill={`${c}2e`}
        stroke={c}
        strokeWidth="1"
        strokeLinejoin="round"
      />
      {open && (
        <path
          d="M3.4 12.6l1.7-5.1a.8.8 0 0 1 .8-.6h9.1l-1.7 5.1a.8.8 0 0 1-.8.6z"
          fill={`${c}40`}
          stroke={c}
          strokeWidth="1"
          strokeLinejoin="round"
        />
      )}
    </svg>
  );
}

export function CategoryTag({ category }: { category: string }) {
  const map: Record<string, string> = {
    file: "#6e7681",
    symbol: "#4c8bf5",
    route: "#3fb950",
    action: "#b07bff",
    setting: "#f0b429",
  };
  return (
    <span
      className="text-[9px] uppercase tracking-wider px-1.5 py-0.5 rounded"
      style={{
        color: map[category] || "#8b949e",
        background: `${map[category] || "#8b949e"}1f`,
      }}
    >
      {category}
    </span>
  );
}
