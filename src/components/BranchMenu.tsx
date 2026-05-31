import { useEffect, useMemo, useState } from "react";
import { api, type BranchInfo } from "../lib/api";

// JetBrains-style branch popover: search, quick actions, and recent / local /
// remote branch lists with per-branch ahead/behind indicators.
export default function BranchMenu({
  onClose,
  onAction,
  onCheckout,
}: {
  onClose: () => void;
  onAction: (action: "update" | "commit" | "push" | "new-branch") => void;
  onCheckout: (branch: string) => void;
}) {
  const [branches, setBranches] = useState<BranchInfo[]>([]);
  const [q, setQ] = useState("");

  useEffect(() => {
    api.gitBranchesDetailed().then(setBranches).catch(() => setBranches([]));
  }, []);

  const { locals, remotes } = useMemo(() => {
    const f = branches.filter((b) =>
      b.name.toLowerCase().includes(q.toLowerCase())
    );
    return {
      locals: f.filter((b) => !b.remote),
      remotes: f.filter((b) => b.remote),
    };
  }, [branches, q]);

  const Track = ({ b }: { b: BranchInfo }) =>
    b.ahead || b.behind ? (
      <span className="text-fg-faint text-[10px] ml-auto shrink-0">
        {b.behind ? `↓${b.behind > 99 ? "99+" : b.behind}` : ""}
        {b.ahead ? ` ↑${b.ahead > 99 ? "99+" : b.ahead}` : ""}
      </span>
    ) : null;

  const Row = ({ b }: { b: BranchInfo }) => (
    <div
      className="flex items-center gap-2 px-3 py-1 hover:bg-bg-hover cursor-pointer text-sm"
      onClick={() => {
        onCheckout(b.remote ? b.name.replace(/^origin\//, "") : b.name);
        onClose();
      }}
      title={b.upstream ? `tracks ${b.upstream}` : undefined}
    >
      <span className={b.current ? "text-warn" : "text-fg-faint"}>
        {b.current ? "★" : b.remote ? "☁" : "⎇"}
      </span>
      <span className={`truncate ${b.current ? "text-fg" : "text-fg-muted"}`}>
        {b.name}
      </span>
      <Track b={b} />
    </div>
  );

  const Action = ({
    label,
    accel,
    action,
  }: {
    label: string;
    accel?: string;
    action: "update" | "commit" | "push" | "new-branch";
  }) => (
    <div
      className="flex items-center gap-2 px-3 py-1.5 hover:bg-bg-hover cursor-pointer text-sm"
      onClick={() => {
        onAction(action);
        onClose();
      }}
    >
      <span className="text-fg-muted">{label}</span>
      {accel && <span className="kbd ml-auto">{accel}</span>}
    </div>
  );

  return (
    <div className="fixed inset-0 z-50" onClick={onClose}>
      <div
        className="pop-in absolute top-9 left-24 w-80 bg-bg-panel border border-border rounded-lg shadow-2xl overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="p-2 border-b border-border">
          <input
            autoFocus
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder="Search for branches and actions"
            className="w-full bg-bg-elevated border border-border rounded px-2 py-1.5 text-sm outline-none focus:border-accent"
          />
        </div>
        <div className="max-h-[60vh] overflow-y-auto py-1">
          <Action label="Update Project…" accel="⌘T" action="update" />
          <Action label="Commit…" accel="⌘K" action="commit" />
          <Action label="Push…" accel="⇧⌘K" action="push" />
          <div className="border-t border-border my-1" />
          <Action label="New Branch…" action="new-branch" />

          {locals.length > 0 && (
            <>
              <div className="panel-title">Local</div>
              {locals.map((b) => (
                <Row key={b.name} b={b} />
              ))}
            </>
          )}
          {remotes.length > 0 && (
            <>
              <div className="panel-title">Remote</div>
              {remotes.slice(0, 40).map((b) => (
                <Row key={b.name} b={b} />
              ))}
            </>
          )}
          {branches.length === 0 && (
            <div className="px-3 py-4 text-fg-faint text-xs">
              Not a git repository.
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
