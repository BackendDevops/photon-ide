import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { api, type DebugBreak, type DebugVariable } from "../lib/api";

// Xdebug control panel: listen, step, inspect stack + variables. Breakpoints
// are toggled in the editor (F9) and listed here.
export default function DebugPanel({
  breakpoints,
  onLocate,
  onToggleBreakpoint,
}: {
  breakpoints: { path: string; line: number; condition?: string }[];
  onLocate: (file: string, line: number) => void;
  onToggleBreakpoint: (path: string, line: number) => void;
}) {
  const [status, setStatus] = useState("idle");
  const [brk, setBrk] = useState<DebugBreak | null>(null);
  const [children, setChildren] = useState<DebugVariable[]>([]);

  useEffect(() => {
    const subs = [
      listen<string>("xdebug-status", (e) => setStatus(e.payload)),
      listen<string>("xdebug-error", (e) => setStatus(`error: ${e.payload}`)),
      listen<string>("xdebug-end", () => { setStatus("finished"); setBrk(null); setChildren([]); }),
      listen<DebugVariable[]>("xdebug-property", (e) => setChildren(e.payload)),
      listen<DebugBreak>("xdebug-break", async (e) => {
        setStatus("paused");
        setChildren([]);
        setBrk(e.payload);
        try {
          const wp = await api.pathToWorkspace(e.payload.file);
          if (wp) onLocate(wp, e.payload.line);
        } catch {
          /* ignore */
        }
      }),
    ];
    return () => { subs.forEach((s) => s.then((u) => u())); };
  }, [onLocate]);

  const ctrl = (verb: "run" | "step_into" | "step_over" | "step_out" | "stop") =>
    api.debugCommand(verb).catch(() => {});

  const Btn = ({ label, onClick, on }: { label: string; onClick: () => void; on?: boolean }) => (
    <button
      onClick={onClick}
      className={`px-2 py-0.5 rounded text-2xs border ${on ? "bg-accent/20 text-accent border-accent/40" : "bg-bg-elevated border-border hover:bg-bg-hover"}`}
    >
      {label}
    </button>
  );

  return (
    <div className="h-full flex flex-col text-sm">
      <div className="px-2 py-2 border-b border-border flex flex-wrap items-center gap-1.5">
        <Btn label="● Listen" onClick={() => { api.debugListen().catch(() => {}); setStatus("listening"); }} />
        <Btn label="▶ Continue" onClick={() => ctrl("run")} />
        <Btn label="↓ Into" onClick={() => ctrl("step_into")} />
        <Btn label="↷ Over" onClick={() => ctrl("step_over")} />
        <Btn label="↑ Out" onClick={() => ctrl("step_out")} />
        <Btn label="■ Stop" onClick={() => { ctrl("stop"); setStatus("idle"); setBrk(null); }} />
        <span className="ml-auto text-2xs text-fg-faint">{status}</span>
      </div>

      <div className="flex-1 overflow-auto">
        {brk && (
          <>
            <div className="panel-title">Variables</div>
            {brk.vars.length === 0 && <div className="px-3 text-2xs text-fg-faint">—</div>}
            {brk.vars.map((v, i) => (
              <div
                key={i}
                className="px-3 py-0.5 text-xs flex gap-2 hover:bg-bg-hover cursor-pointer"
                title="Click to expand children"
                onClick={() => api.debugProperty(v.name).catch(() => {})}
              >
                <span className="text-[#c8a3e0]">{v.name}</span>
                <span className="text-fg-faint">{v.ty}</span>
                <span className="text-fg truncate flex-1" title={v.value}>{v.value}</span>
              </div>
            ))}
            {children.length > 0 && (
              <>
                <div className="panel-title">Expanded</div>
                {children.map((v, i) => (
                  <div key={i} className="pl-6 pr-3 py-0.5 text-xs flex gap-2">
                    <span className="text-[#c8a3e0]">{v.name}</span>
                    <span className="text-fg-faint">{v.ty}</span>
                    <span className="text-fg truncate flex-1" title={v.value}>{v.value}</span>
                  </div>
                ))}
              </>
            )}
            <div className="panel-title">Call stack</div>
            {brk.stack.map((f, i) => (
              <div
                key={i}
                className="px-3 py-0.5 text-xs flex gap-2 hover:bg-bg-hover cursor-pointer"
                onClick={() => api.pathToWorkspace(f.file).then((wp) => wp && onLocate(wp, f.line)).catch(() => {})}
              >
                <span className="text-fg truncate flex-1">{f.func || "{main}"}</span>
                <span className="text-fg-faint">{f.file.split("/").pop()}:{f.line}</span>
              </div>
            ))}
          </>
        )}
        <div className="panel-title">Breakpoints ({breakpoints.length})</div>
        {breakpoints.length === 0 && (
          <div className="px-3 text-2xs text-fg-faint">Toggle with F9 on a line.</div>
        )}
        {breakpoints.map((b, i) => (
          <div key={i} className="px-3 py-0.5 text-xs flex items-center gap-2 group">
            <span className="text-danger">●</span>
            <span
              className="text-fg truncate flex-1 cursor-pointer"
              onClick={() => onLocate(b.path, b.line)}
              title={b.condition ? `if (${b.condition})` : undefined}
            >
              {b.path.split("/").pop()}:{b.line}
              {b.condition && <span className="text-[#d29922] ml-1">?</span>}
            </span>
            <button
              className="opacity-0 group-hover:opacity-100 text-fg-faint hover:text-fg"
              onClick={() => onToggleBreakpoint(b.path, b.line)}
            >
              ✕
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
