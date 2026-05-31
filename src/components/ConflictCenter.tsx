import { useCallback, useEffect, useMemo, useState } from "react";
import { api, type ConflictVersions } from "../lib/api";

type Choice = "ours" | "theirs" | "both" | "both-rev" | null;

type Seg =
  | { type: "text"; lines: string[] }
  | { type: "conflict"; ours: string[]; theirs: string[]; base: string[] };

// Split a marker-laden working file into ordered text/conflict segments.
function parseConflicts(working: string): Seg[] {
  const lines = working.split("\n");
  const segs: Seg[] = [];
  let text: string[] = [];
  let i = 0;
  const flush = () => {
    if (text.length) {
      segs.push({ type: "text", lines: text });
      text = [];
    }
  };
  while (i < lines.length) {
    const l = lines[i];
    if (l.startsWith("<<<<<<<")) {
      flush();
      const ours: string[] = [];
      const base: string[] = [];
      const theirs: string[] = [];
      i++;
      while (i < lines.length && !lines[i].startsWith("|||||||") && !lines[i].startsWith("=======")) {
        ours.push(lines[i]);
        i++;
      }
      if (i < lines.length && lines[i].startsWith("|||||||")) {
        i++;
        while (i < lines.length && !lines[i].startsWith("=======")) {
          base.push(lines[i]);
          i++;
        }
      }
      if (i < lines.length && lines[i].startsWith("=======")) i++;
      while (i < lines.length && !lines[i].startsWith(">>>>>>>")) {
        theirs.push(lines[i]);
        i++;
      }
      if (i < lines.length && lines[i].startsWith(">>>>>>>")) i++;
      segs.push({ type: "conflict", ours, theirs, base });
    } else {
      text.push(l);
      i++;
    }
  }
  flush();
  return segs;
}

function buildResult(segs: Seg[], choices: Choice[]): string {
  const out: string[] = [];
  let ci = 0;
  for (const s of segs) {
    if (s.type === "text") {
      out.push(...s.lines);
      continue;
    }
    const choice = choices[ci];
    ci++;
    if (choice === "ours") out.push(...s.ours);
    else if (choice === "theirs") out.push(...s.theirs);
    else if (choice === "both") out.push(...s.ours, ...s.theirs);
    else if (choice === "both-rev") out.push(...s.theirs, ...s.ours);
    else {
      // Unresolved → keep markers so nothing is silently lost.
      out.push("<<<<<<< ours", ...s.ours, "=======", ...s.theirs, ">>>>>>> theirs");
    }
  }
  return out.join("\n");
}

// Three-way conflict resolution center (docs/16 §9). Per-conflict accept
// ours/theirs/both with a live merged preview, then stage the resolved file.
export default function ConflictCenter({
  file,
  onClose,
  onResolved,
}: {
  file: string;
  onClose: () => void;
  onResolved: () => void;
}) {
  const [data, setData] = useState<ConflictVersions | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [choices, setChoices] = useState<Choice[]>([]);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      const v = await api.gitConflictVersions(file);
      setData(v);
      const n = parseConflicts(v.working).filter((s) => s.type === "conflict").length;
      setChoices(Array(n).fill(null));
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }, [file]);

  useEffect(() => {
    void load();
  }, [load]);

  const segs = useMemo(() => (data ? parseConflicts(data.working) : []), [data]);
  const conflictCount = segs.filter((s) => s.type === "conflict").length;
  const resolvedCount = choices.filter((c) => c !== null).length;
  const allResolved = conflictCount > 0 && resolvedCount === conflictCount;

  const setChoice = (idx: number, c: Choice) =>
    setChoices((prev) => prev.map((x, i) => (i === idx ? c : x)));
  const setAll = (c: Choice) => setChoices((prev) => prev.map(() => c));

  const markResolved = async () => {
    if (!data) return;
    setBusy(true);
    try {
      await api.gitResolveContent(file, buildResult(segs, choices));
      onResolved();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const oursLabel = data?.ours_label || "ours";
  const theirsLabel = data?.theirs_label || "incoming";

  let cIdx = -1;

  return (
    <div className="flex-1 flex flex-col min-h-0">
      <div className="h-9 shrink-0 flex items-center gap-2 px-3 bg-bg-panel border-b border-border text-sm">
        <button
          onClick={onClose}
          className="text-fg-faint hover:text-fg text-xs px-1.5 py-0.5 rounded hover:bg-bg-hover"
          title="Back to commit graph"
        >
          ← Back
        </button>
        <span className="text-fg-muted truncate">Resolve · {file.split("/").pop()}</span>
        <span className="text-fg-faint text-xs">
          {resolvedCount}/{conflictCount} resolved
        </span>
        <div className="ml-auto flex items-center gap-1.5 text-xs">
          <button
            onClick={() => setAll("ours")}
            className="px-2 py-0.5 rounded bg-bg-elevated border border-border hover:bg-bg-hover"
          >
            Accept all ours
          </button>
          <button
            onClick={() => setAll("theirs")}
            className="px-2 py-0.5 rounded bg-bg-elevated border border-border hover:bg-bg-hover"
          >
            Accept all theirs
          </button>
        </div>
      </div>

      {error && <div className="px-3 py-2 text-danger text-xs">{error}</div>}

      <div className="flex-1 overflow-auto p-3 space-y-2 font-mono text-[11px] leading-tight">
        {segs.map((s, i) => {
          if (s.type === "text") {
            const lines = s.lines;
            // Collapse long unchanged context.
            const show =
              lines.length > 8
                ? [...lines.slice(0, 3), `   … ${lines.length - 6} unchanged lines …`, ...lines.slice(-3)]
                : lines;
            return (
              <pre key={i} className="text-fg-faint whitespace-pre-wrap m-0">
                {show.join("\n")}
              </pre>
            );
          }
          cIdx++;
          const idx = cIdx;
          const choice = choices[idx];
          const Side = ({
            who,
            label,
            color,
            lines,
            pick,
          }: {
            who: Choice;
            label: string;
            color: string;
            lines: string[];
            pick: Choice;
          }) => (
            <div
              className="flex-1 rounded border overflow-hidden cursor-pointer"
              style={{
                borderColor: choice === pick ? color : "var(--line)",
                boxShadow: choice === pick ? `inset 0 0 0 1px ${color}` : "none",
              }}
              onClick={() => setChoice(idx, choice === pick ? null : pick)}
            >
              <div
                className="flex items-center justify-between px-2 py-1 text-2xs"
                style={{ background: `${color}22`, color }}
              >
                <span className="truncate">{label}</span>
                {choice === pick && <span>✓</span>}
              </div>
              <pre className="m-0 p-1.5 max-h-48 overflow-auto whitespace-pre-wrap" style={{ color }}>
                {lines.length ? lines.join("\n") : "(empty)"}
              </pre>
              <div className="px-2 py-0.5 text-2xs text-fg-faint border-t border-line/50">
                {who === "ours" ? "Use ours" : "Use theirs"}
              </div>
            </div>
          );
          return (
            <div key={i} className="rounded-md border border-line bg-bg-elevated/40 p-1.5">
              <div className="flex gap-1.5">
                <Side who="ours" pick="ours" label={`OURS · ${oursLabel}`} color="#3fb950" lines={s.ours} />
                <Side who="theirs" pick="theirs" label={`THEIRS · ${theirsLabel}`} color="#4c8bf5" lines={s.theirs} />
              </div>
              <div className="flex items-center gap-1.5 mt-1.5 text-2xs">
                <button
                  onClick={() => setChoice(idx, "both")}
                  className={`px-2 py-0.5 rounded border ${choice === "both" ? "border-accent text-accent" : "border-line text-fg-faint hover:text-fg"}`}
                >
                  Both (ours→theirs)
                </button>
                <button
                  onClick={() => setChoice(idx, "both-rev")}
                  className={`px-2 py-0.5 rounded border ${choice === "both-rev" ? "border-accent text-accent" : "border-line text-fg-faint hover:text-fg"}`}
                >
                  Both (theirs→ours)
                </button>
                {choice && (
                  <button
                    onClick={() => setChoice(idx, null)}
                    className="px-2 py-0.5 rounded text-fg-faint hover:text-fg ml-auto"
                  >
                    Clear
                  </button>
                )}
              </div>
            </div>
          );
        })}
        {conflictCount === 0 && (
          <div className="text-fg-faint">No conflict markers found in this file.</div>
        )}
      </div>

      <div className="border-t border-border p-2 flex items-center gap-2">
        <div className="text-2xs text-fg-faint">
          {allResolved ? "All conflicts resolved." : `${conflictCount - resolvedCount} conflict(s) remaining.`}
        </div>
        <div className="ml-auto flex gap-1.5">
          <button
            onClick={() => void api.gitResolveContent(file, buildResult(segs, choices)).then(onResolved).catch((e) => setError(String(e)))}
            disabled={busy}
            className="text-xs px-2.5 py-1 rounded bg-bg-elevated border border-border hover:bg-bg-hover"
            title="Save the current merged result without requiring every conflict chosen"
          >
            Save draft
          </button>
          <button
            onClick={markResolved}
            disabled={busy || !allResolved}
            className="text-xs px-3 py-1 rounded bg-accent text-white hover:bg-accent-hover disabled:opacity-40"
          >
            Mark resolved & stage
          </button>
        </div>
      </div>
    </div>
  );
}
