import { useMemo } from "react";

// Roots that belong to the framework / common vendor packages — drawn "cool".
const FRAMEWORK_ROOTS = new Set([
  "Illuminate",
  "Symfony",
  "Psr",
  "Laravel",
  "Doctrine",
  "Monolog",
  "Carbon",
  "GuzzleHttp",
  "League",
  "Ramsey",
  "PHPUnit",
  "Pest",
  "Faker",
]);

interface DepNode {
  short: string;
  fqn: string;
  kind: "app" | "framework" | "php";
}

interface Graph {
  center: string;
  ns: string | null;
  nodes: DepNode[];
}

function parseGraph(code: string, fileName: string): Graph {
  const ns = code.match(/namespace\s+([\w\\]+)\s*;/)?.[1] ?? null;
  const cls =
    code.match(/(?:^|\n)\s*(?:final\s+|abstract\s+|readonly\s+)*(?:class|interface|trait|enum)\s+(\w+)/)?.[1] ??
    fileName.split("/").pop()?.replace(/\.php$/, "") ??
    "current";

  const fqns = new Set<string>();
  // use A\B\C;  /  use function A\b;  /  use A\B as C;
  const useRe = /use\s+(?:function\s+|const\s+)?([\w\\]+)(?:\s+as\s+\w+)?\s*;/g;
  let m: RegExpExecArray | null;
  while ((m = useRe.exec(code))) fqns.add(m[1]);
  // grouped: use A\B\{C, D as E, F};
  const groupRe = /use\s+([\w\\]+)\\\{([^}]+)\}/g;
  while ((m = groupRe.exec(code))) {
    const base = m[1];
    for (const part of m[2].split(",")) {
      const name = part.trim().split(/\s+as\s+/)[0].trim();
      if (name) fqns.add(`${base}\\${name}`);
    }
  }

  const nodes: DepNode[] = [...fqns].map((fqn) => {
    const segs = fqn.split("\\").filter(Boolean);
    const root = segs[0] ?? fqn;
    const short = segs[segs.length - 1] ?? fqn;
    const kind: DepNode["kind"] = segs.length <= 1
      ? "php"
      : FRAMEWORK_ROOTS.has(root)
      ? "framework"
      : "app";
    return { short, fqn, kind };
  });
  nodes.sort((a, b) => a.kind.localeCompare(b.kind) || a.short.localeCompare(b.short));
  return { center: cls, ns, nodes };
}

const COLOR: Record<DepNode["kind"], string> = {
  app: "#3574f0",
  framework: "#a371f7",
  php: "#6b7280",
};

export default function ArchMap({
  code,
  fileName,
  onPick,
}: {
  code: string | null;
  fileName: string | null;
  onPick: (name: string) => void;
}) {
  const graph = useMemo(
    () => (code ? parseGraph(code, fileName ?? "") : null),
    [code, fileName]
  );

  if (!graph) {
    return (
      <div className="h-full flex items-center justify-center text-fg-faint text-xs px-4 text-center">
        Open a PHP file to see its dependency map.
      </div>
    );
  }

  const shown = graph.nodes.slice(0, 16);
  const overflow = graph.nodes.length - shown.length;
  const W = 300;
  const H = 300;
  const cx = W / 2;
  const cy = H / 2;
  const R = 112;

  const counts = {
    app: graph.nodes.filter((n) => n.kind === "app").length,
    framework: graph.nodes.filter((n) => n.kind === "framework").length,
    php: graph.nodes.filter((n) => n.kind === "php").length,
  };

  return (
    <div className="h-full flex flex-col text-xs">
      <div className="px-3 py-2 border-b border-line">
        <div className="text-fg font-medium truncate">{graph.center}</div>
        {graph.ns && <div className="text-fg-faint truncate text-2xs">{graph.ns}</div>}
      </div>

      <div className="flex-1 min-h-0 overflow-auto flex items-center justify-center p-2">
        {shown.length === 0 ? (
          <div className="text-fg-faint">No imports — this file stands alone.</div>
        ) : (
          <svg viewBox={`0 0 ${W} ${H}`} className="w-full max-w-[320px]">
            {shown.map((n, i) => {
              const a = (i / shown.length) * Math.PI * 2 - Math.PI / 2;
              const x = cx + Math.cos(a) * R;
              const y = cy + Math.sin(a) * R;
              return (
                <line
                  key={`l${i}`}
                  x1={cx}
                  y1={cy}
                  x2={x}
                  y2={y}
                  stroke={COLOR[n.kind]}
                  strokeOpacity="0.35"
                  strokeWidth="1.2"
                />
              );
            })}
            {/* center node */}
            <circle cx={cx} cy={cy} r="22" fill="#3574f0" fillOpacity="0.18" stroke="#3574f0" strokeWidth="1.5" />
            <text x={cx} y={cy + 3} textAnchor="middle" fill="#e6e6e6" fontSize="9" fontWeight="600">
              {graph.center.slice(0, 8)}
            </text>
            {/* dependency nodes */}
            {shown.map((n, i) => {
              const a = (i / shown.length) * Math.PI * 2 - Math.PI / 2;
              const x = cx + Math.cos(a) * R;
              const y = cy + Math.sin(a) * R;
              return (
                <g
                  key={`n${i}`}
                  className="cursor-pointer"
                  onClick={() => onPick(n.short)}
                >
                  <title>{n.fqn}</title>
                  <circle cx={x} cy={y} r="6" fill={COLOR[n.kind]} />
                  <text
                    x={x}
                    y={y - 10}
                    textAnchor="middle"
                    fill="#9aa3b2"
                    fontSize="7.5"
                  >
                    {n.short.length > 14 ? n.short.slice(0, 13) + "…" : n.short}
                  </text>
                </g>
              );
            })}
          </svg>
        )}
      </div>

      <div className="px-3 py-2 border-t border-line flex items-center gap-3 text-2xs text-fg-faint">
        <span className="flex items-center gap-1">
          <span className="w-2 h-2 rounded-full" style={{ background: COLOR.app }} /> app {counts.app}
        </span>
        <span className="flex items-center gap-1">
          <span className="w-2 h-2 rounded-full" style={{ background: COLOR.framework }} /> framework {counts.framework}
        </span>
        <span className="flex items-center gap-1">
          <span className="w-2 h-2 rounded-full" style={{ background: COLOR.php }} /> php {counts.php}
        </span>
        {overflow > 0 && <span className="ml-auto">+{overflow} more</span>}
      </div>
    </div>
  );
}
