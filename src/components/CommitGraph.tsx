import { useEffect, useMemo, useRef, useState } from "react";
import { api, type GraphCommit } from "../lib/api";
import ContextMenu, { type MenuItem } from "./ContextMenu";

type DragPayload =
  | { kind: "commit"; hash: string; short: string }
  | { kind: "branch"; name: string; current: boolean };

const LANE_COLORS = [
  "#4c8bf5",
  "#3fb950",
  "#d29922",
  "#9d6cff",
  "#f85149",
  "#56b6c2",
  "#e5a13a",
  "#db61a2",
];

const ROW_H = 28;
const LANE_W = 16;
const PAD_X = 14;

// Author initials → a stable avatar color (no network avatars in v1).
function avatar(name: string): { initials: string; color: string } {
  const initials = name
    .split(/\s+/)
    .map((p) => p[0])
    .filter(Boolean)
    .slice(0, 2)
    .join("")
    .toUpperCase();
  let h = 0;
  for (const c of name) h = (h * 31 + c.charCodeAt(0)) % 360;
  return { initials: initials || "?", color: `hsl(${h} 45% 45%)` };
}

function refChip(ref: string) {
  const isHead = ref.includes("HEAD");
  const isTag = ref.startsWith("tag:");
  const isRemote = ref.startsWith("origin/");
  const label = ref.replace("tag: ", "").replace("HEAD -> ", "");
  const color = isHead
    ? "#3fb950"
    : isTag
    ? "#d29922"
    : isRemote
    ? "#8b949e"
    : "#4c8bf5";
  return { label, color, isTag };
}

// The interactive commit graph — GitKraken-inspired lanes & nodes.
// (docs/16-git-experience.md — visual commit graph)
export default function CommitGraph({
  refreshKey,
  onPickCommit,
  onChanged,
  currentBranch,
  onRebaseFrom,
}: {
  refreshKey: number;
  onPickCommit?: (hash: string) => void;
  onChanged?: () => void;
  currentBranch?: string | null;
  onRebaseFrom?: (hash: string) => void;
}) {
  const [commits, setCommits] = useState<GraphCommit[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [menu, setMenu] = useState<{ x: number; y: number; hash: string; short: string } | null>(null);
  const [flash, setFlash] = useState<string | null>(null);
  const drag = useRef<DragPayload | null>(null);
  const [dropHash, setDropHash] = useState<string | null>(null);
  const [dropChip, setDropChip] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const [view, setView] = useState({ top: 0, height: 900 });

  useEffect(() => {
    if (!flash) return;
    const t = setTimeout(() => setFlash(null), 2800);
    return () => clearTimeout(t);
  }, [flash]);

  const runAction = (label: string, fn: () => Promise<unknown>) => {
    setMenu(null);
    fn()
      .then(() => {
        setFlash(`${label} ✓`);
        onChanged?.();
      })
      .catch((e) => setFlash(String(e)));
  };

  // Drop a dragged branch/commit onto a commit node.
  const dropOnCommit = (hash: string, short: string) => {
    const p = drag.current;
    drag.current = null;
    setDropHash(null);
    if (!p || p.kind !== "branch") return;
    if (p.current) {
      if (confirm(`Reset "${p.name}" to ${short}? Changes are kept (unstaged).`))
        runAction(`Reset ${p.name} → ${short}`, () => api.gitReset(hash, "mixed"));
    } else if (confirm(`Move branch "${p.name}" to ${short}?`)) {
      runAction(`Moved ${p.name} → ${short}`, () => api.gitBranchForce(p.name, hash));
    }
  };

  // Drop a dragged branch/commit onto a branch chip.
  const dropOnChip = (name: string, current: boolean) => {
    const p = drag.current;
    drag.current = null;
    setDropChip(null);
    if (!p) return;
    if (p.kind === "commit") {
      if (!confirm(`Cherry-pick ${p.short} onto "${name}"?`)) return;
      runAction(`Cherry-picked ${p.short} → ${name}`, async () => {
        if (!current) await api.gitCheckout(name);
        await api.gitCherryPick(p.hash);
      });
    } else if (p.kind === "branch" && p.name !== name) {
      if (!confirm(`Merge "${p.name}" into "${name}"?`)) return;
      runAction(`Merged ${p.name} → ${name}`, async () => {
        if (!current) await api.gitCheckout(name);
        await api.gitMerge(p.name);
      });
    }
  };

  useEffect(() => {
    api
      .gitGraph(500)
      .then((c) => {
        setCommits(c);
        setError(null);
      })
      .catch((e) => setError(String(e)));
  }, [refreshKey]);

  const { rowOf, maxLane } = useMemo(() => {
    const rowOf = new Map<string, number>();
    let maxLane = 0;
    commits.forEach((c, i) => {
      rowOf.set(c.hash, i);
      if (c.lane > maxLane) maxLane = c.lane;
    });
    return { rowOf, maxLane };
  }, [commits]);

  const graphWidth = (maxLane + 1) * LANE_W + PAD_X;
  const totalHeight = commits.length * ROW_H;

  const laneX = (lane: number) => PAD_X / 2 + lane * LANE_W + LANE_W / 2;

  // Virtualization: only render the rows in (and just around) the viewport.
  const BUF = 16;
  const visStart = Math.max(0, Math.floor(view.top / ROW_H) - BUF);
  const visEnd = Math.min(commits.length, Math.ceil((view.top + view.height) / ROW_H) + BUF);
  const visible = (i: number) => i >= visStart && i < visEnd;

  if (error) {
    return (
      <div className="flex-1 flex items-center justify-center text-fg-faint text-sm flex-col gap-2">
        <div>No git history.</div>
        <div className="text-xs">{error}</div>
      </div>
    );
  }

  return (
    <div
      ref={scrollRef}
      className="flex-1 overflow-auto relative"
      onScroll={(e) => setView({ top: e.currentTarget.scrollTop, height: e.currentTarget.clientHeight })}
    >
      <div className="relative" style={{ height: totalHeight }}>
        {/* edges + nodes */}
        <svg
          width={graphWidth}
          height={totalHeight}
          className="absolute top-0 left-0"
          style={{ pointerEvents: "none" }}
        >
          {commits.map((c, i) => {
            if (!visible(i)) return null;
            const cx = laneX(c.lane);
            const cy = i * ROW_H + ROW_H / 2;
            return c.parents.map((p) => {
              const pr = rowOf.get(p);
              if (pr === undefined) return null;
              const parent = commits[pr];
              const px = laneX(parent.lane);
              const py = pr * ROW_H + ROW_H / 2;
              const midY = (cy + py) / 2;
              const color = LANE_COLORS[c.color % LANE_COLORS.length];
              return (
                <path
                  key={`${c.hash}-${p}`}
                  d={`M ${cx} ${cy} C ${cx} ${midY}, ${px} ${midY}, ${px} ${py}`}
                  stroke={color}
                  strokeWidth={1.6}
                  fill="none"
                  opacity={0.8}
                />
              );
            });
          })}
          {commits.map((c, i) => {
            if (!visible(i)) return null;
            const cx = laneX(c.lane);
            const cy = i * ROW_H + ROW_H / 2;
            const color = LANE_COLORS[c.color % LANE_COLORS.length];
            const isMerge = c.parents.length > 1;
            return (
              <circle
                key={c.hash}
                cx={cx}
                cy={cy}
                r={isMerge ? 5.5 : 4.5}
                fill={selected === c.hash ? "#fff" : color}
                stroke={color}
                strokeWidth={2}
              />
            );
          })}
        </svg>

        {/* commit rows (text) — virtualized with spacers */}
        <div className="absolute top-0" style={{ left: graphWidth, right: 0 }}>
          <div style={{ height: visStart * ROW_H }} />
          {commits.slice(visStart, visEnd).map((c) => {
            const av = avatar(c.author);
            return (
              <div
                key={c.hash}
                draggable
                className={`flex items-center gap-2 px-2 cursor-pointer ${
                  dropHash === c.hash
                    ? "ring-1 ring-accent bg-accent/10"
                    : selected === c.hash
                      ? "bg-accent/15"
                      : "hover:bg-bg-hover"
                }`}
                style={{ height: ROW_H }}
                onClick={() => {
                  setSelected(c.hash);
                  onPickCommit?.(c.hash);
                }}
                onDragStart={(e) => {
                  drag.current = { kind: "commit", hash: c.hash, short: c.short };
                  e.dataTransfer.effectAllowed = "copyMove";
                }}
                onDragOver={(e) => {
                  if (drag.current?.kind === "branch") {
                    e.preventDefault();
                    setDropHash(c.hash);
                  }
                }}
                onDragLeave={() => setDropHash((h) => (h === c.hash ? null : h))}
                onDrop={(e) => {
                  e.preventDefault();
                  dropOnCommit(c.hash, c.short);
                }}
                onContextMenu={(e) => {
                  e.preventDefault();
                  setSelected(c.hash);
                  setMenu({ x: e.clientX, y: e.clientY, hash: c.hash, short: c.short });
                }}
              >
                <span
                  className="inline-flex items-center justify-center w-5 h-5 rounded-full text-[9px] font-bold text-white shrink-0"
                  style={{ background: av.color }}
                  title={`${c.author} <${c.email}>`}
                >
                  {av.initials}
                </span>
                {c.refs.map((r) => {
                  const chip = refChip(r);
                  const isRemote = r.startsWith("origin/") || r.includes("/");
                  // A draggable local branch: not a tag, not a remote-only ref.
                  const isBranch = !chip.isTag && !r.startsWith("tag:") && !isRemote;
                  const isCurrent = r.includes("HEAD ->") || chip.label === currentBranch;
                  const dz = dropChip === `${c.hash}:${chip.label}`;
                  return (
                    <span
                      key={r}
                      draggable={isBranch}
                      className="text-[10px] px-1.5 py-0.5 rounded-full whitespace-nowrap shrink-0"
                      style={{
                        color: chip.color,
                        background: dz ? `${chip.color}44` : `${chip.color}22`,
                        border: `1px solid ${dz ? chip.color : `${chip.color}55`}`,
                        cursor: isBranch ? "grab" : undefined,
                      }}
                      title={isBranch ? "Drag onto a commit (move/reset) or another branch (merge)" : undefined}
                      onDragStart={
                        isBranch
                          ? (e) => {
                              e.stopPropagation();
                              drag.current = { kind: "branch", name: chip.label, current: isCurrent };
                              e.dataTransfer.effectAllowed = "move";
                            }
                          : undefined
                      }
                      onDragOver={
                        isBranch
                          ? (e) => {
                              e.preventDefault();
                              e.stopPropagation();
                              setDropChip(`${c.hash}:${chip.label}`);
                            }
                          : undefined
                      }
                      onDragLeave={isBranch ? () => setDropChip(null) : undefined}
                      onDrop={
                        isBranch
                          ? (e) => {
                              e.preventDefault();
                              e.stopPropagation();
                              dropOnChip(chip.label, isCurrent);
                            }
                          : undefined
                      }
                    >
                      {chip.isTag ? "🏷 " : ""}
                      {chip.label}
                    </span>
                  );
                })}
                <span className="truncate text-fg text-sm flex-1">
                  {c.subject}
                </span>
                <span className="text-fg-faint text-xs shrink-0 hidden sm:inline">
                  {c.author}
                </span>
                <span className="text-fg-faint text-xs shrink-0 font-mono">
                  {c.short}
                </span>
                <span className="text-fg-faint text-xs shrink-0">{c.date}</span>
              </div>
            );
          })}
          <div style={{ height: Math.max(0, (commits.length - visEnd) * ROW_H) }} />
        </div>
      </div>

      {flash && (
        <div className="absolute top-2 left-1/2 -translate-x-1/2 z-20 px-3 py-1.5 rounded-md bg-bg-elevated border border-border text-xs text-fg shadow-lg max-w-[80%] truncate">
          {flash}
        </div>
      )}

      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          onClose={() => setMenu(null)}
          items={
            [
              {
                label: "Cherry-pick onto current branch",
                icon: "⑂",
                onClick: () => runAction(`Cherry-picked ${menu.short}`, () => api.gitCherryPick(menu.hash)),
              },
              {
                label: "Revert this commit",
                icon: "↶",
                onClick: () => runAction(`Reverted ${menu.short}`, () => api.gitRevert(menu.hash)),
              },
              {
                label: "Interactive rebase from here…",
                icon: "≡",
                onClick: () => {
                  const h = menu.hash;
                  setMenu(null);
                  onRebaseFrom?.(h);
                },
              },
              { separator: true },
              {
                label: "Reset — soft (keep changes staged)",
                icon: "⟲",
                onClick: () => runAction(`Reset (soft) to ${menu.short}`, () => api.gitReset(menu.hash, "soft")),
              },
              {
                label: "Reset — mixed (keep changes unstaged)",
                icon: "⟲",
                onClick: () => runAction(`Reset (mixed) to ${menu.short}`, () => api.gitReset(menu.hash, "mixed")),
              },
              {
                label: "Reset — hard (discard changes)",
                icon: "⟲",
                danger: true,
                onClick: () => runAction(`Reset (hard) to ${menu.short}`, () => api.gitReset(menu.hash, "hard")),
              },
              { separator: true },
              {
                label: "Copy commit hash",
                icon: "⧉",
                onClick: () => {
                  void navigator.clipboard?.writeText(menu.hash);
                  setMenu(null);
                  setFlash("Hash copied ✓");
                },
              },
            ] as MenuItem[]
          }
        />
      )}
    </div>
  );
}
