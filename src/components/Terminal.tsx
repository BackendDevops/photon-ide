import { useEffect, useRef, useState } from "react";
import { Terminal as XTerm } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import "@xterm/xterm/css/xterm.css";
import { api } from "../lib/api";

const THEME = {
  background: "#0b0d11",
  foreground: "#e4e8ef",
  cursor: "#5b8cff",
  black: "#0d0f12",
  brightBlack: "#3a414c",
  red: "#f85149",
  green: "#3fb950",
  yellow: "#d29922",
  blue: "#4c8bf5",
  magenta: "#9d6cff",
  cyan: "#56b6c2",
  white: "#c9d1d9",
};

// One xterm bound to a backend PTY session. The PTY streams output via the
// `term-data-<id>` event; keystrokes go back through `term_write`.
function TerminalView({ cwd, visible }: { cwd: string | null; visible: boolean }) {
  const hostRef = useRef<HTMLDivElement>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const idRef = useRef<string | null>(null);
  const termRef = useRef<XTerm | null>(null);

  useEffect(() => {
    if (!hostRef.current) return;
    let unlistenData: UnlistenFn | null = null;
    let unlistenExit: UnlistenFn | null = null;
    let disposed = false;

    const term = new XTerm({
      fontFamily: "JetBrains Mono, SFMono-Regular, Menlo, monospace",
      fontSize: 12,
      theme: THEME,
      cursorBlink: true,
      scrollback: 5000,
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(hostRef.current);
    fit.fit();
    termRef.current = term;
    fitRef.current = fit;

    (async () => {
      const id = await api.termSpawn(cwd, term.cols, term.rows);
      if (disposed) {
        await api.termKill(id);
        return;
      }
      idRef.current = id;
      unlistenData = await listen<string>(`term-data-${id}`, (e) => {
        term.write(e.payload);
      });
      unlistenExit = await listen(`term-exit-${id}`, () => {
        term.write("\r\n\x1b[90m[process exited]\x1b[0m\r\n");
      });
      term.onData((d) => api.termWrite(id, d));
    })();

    const ro = new ResizeObserver(() => {
      try {
        fit.fit();
        if (idRef.current) api.termResize(idRef.current, term.cols, term.rows);
      } catch {
        /* not visible */
      }
    });
    ro.observe(hostRef.current);

    return () => {
      disposed = true;
      ro.disconnect();
      unlistenData?.();
      unlistenExit?.();
      if (idRef.current) void api.termKill(idRef.current);
      term.dispose();
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Re-fit when this tab becomes visible.
  useEffect(() => {
    if (visible && fitRef.current && idRef.current) {
      requestAnimationFrame(() => {
        try {
          fitRef.current?.fit();
          const t = termRef.current;
          if (t && idRef.current) api.termResize(idRef.current, t.cols, t.rows);
        } catch {
          /* noop */
        }
      });
    }
  }, [visible]);

  return <div ref={hostRef} className="w-full h-full" style={{ padding: 4 }} />;
}

// Bottom terminal dock with tabs. Sessions stay mounted (alive) when hidden.
export default function TerminalDock({
  cwd,
  onClose,
}: {
  cwd: string | null;
  onClose: () => void;
}) {
  const [tabs, setTabs] = useState<number[]>([0]);
  const [active, setActive] = useState(0);
  const nextId = useRef(1);

  const add = () => {
    const id = nextId.current++;
    setTabs((t) => [...t, id]);
    setActive(id);
  };
  const close = (id: number) => {
    setTabs((t) => {
      const next = t.filter((x) => x !== id);
      if (active === id && next.length) setActive(next[next.length - 1]);
      if (next.length === 0) onClose();
      return next;
    });
  };

  return (
    <div className="h-full flex flex-col">
      <div className="flex items-center gap-1 px-2 py-1 border-b border-border bg-bg-panel">
        <span className="text-fg-faint text-xs mr-2">Terminal</span>
        {tabs.map((id, i) => (
          <div
            key={id}
            onClick={() => setActive(id)}
            className={`group flex items-center gap-1.5 px-2 py-0.5 rounded text-xs cursor-pointer ${
              active === id ? "bg-bg text-fg" : "text-fg-muted hover:bg-bg-hover"
            }`}
          >
            <span>zsh {i + 1}</span>
            <span
              onClick={(e) => {
                e.stopPropagation();
                close(id);
              }}
              className="opacity-0 group-hover:opacity-100 text-fg-faint hover:text-fg"
            >
              ✕
            </span>
          </div>
        ))}
        <button
          onClick={add}
          className="text-fg-faint hover:text-fg px-1.5"
          title="New terminal"
        >
          +
        </button>
        <div className="flex-1" />
        <button
          onClick={onClose}
          className="text-fg-faint hover:text-fg text-xs px-1"
          title="Close panel"
        >
          ✕
        </button>
      </div>
      <div className="flex-1 relative min-h-0">
        {tabs.map((id) => (
          <div
            key={id}
            className="absolute inset-0"
            style={{ visibility: active === id ? "visible" : "hidden" }}
          >
            <TerminalView cwd={cwd} visible={active === id} />
          </div>
        ))}
      </div>
    </div>
  );
}
