import { useState } from "react";
import { api } from "../lib/api";

// Minimal Redis console (NoSQL power tool): connect to a URL, run commands,
// see replies. Connection is keyed by a fixed name for this lightweight tool.
const NAME = "console";

export default function RedisConsole({ onClose }: { onClose: () => void }) {
  const [url, setUrl] = useState("redis://127.0.0.1:6379");
  const [connected, setConnected] = useState(false);
  const [cmd, setCmd] = useState("");
  const [log, setLog] = useState<{ q: string; r: string; err?: boolean }[]>([]);
  const [busy, setBusy] = useState(false);

  const connect = async () => {
    setBusy(true);
    try {
      const pong = await api.redisConnect(NAME, url);
      setConnected(true);
      setLog((l) => [{ q: `connect ${url}`, r: pong }, ...l]);
    } catch (e) {
      setLog((l) => [{ q: `connect ${url}`, r: String(e), err: true }, ...l]);
    } finally {
      setBusy(false);
    }
  };

  const run = async () => {
    const parts = cmd.trim().split(/\s+/).filter(Boolean);
    if (parts.length === 0) return;
    setBusy(true);
    try {
      const r = await api.redisCommand(NAME, parts);
      setLog((l) => [{ q: cmd, r }, ...l]);
      setCmd("");
    } catch (e) {
      setLog((l) => [{ q: cmd, r: String(e), err: true }, ...l]);
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50" onClick={onClose}>
      <div
        className="w-[620px] h-[70vh] flex flex-col rounded-lg border border-border bg-bg-panel shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-4 py-2.5 border-b border-border flex items-center gap-2 text-sm">
          <span className="text-[#dc382d]">◆</span>
          <span className="text-fg-muted">Redis console</span>
          <span className={`text-2xs ${connected ? "text-success" : "text-fg-faint"}`}>
            {connected ? "connected" : "disconnected"}
          </span>
          <button onClick={onClose} className="ml-auto text-fg-faint hover:text-fg">✕</button>
        </div>

        <div className="px-3 py-2 border-b border-border flex items-center gap-2">
          <input
            value={url}
            onChange={(e) => setUrl(e.target.value)}
            placeholder="redis://127.0.0.1:6379"
            className="flex-1 bg-bg-elevated border border-border rounded px-2 py-1 text-xs outline-none focus:border-accent"
          />
          <button
            onClick={connect}
            disabled={busy}
            className="text-xs px-3 py-1 rounded bg-accent text-white hover:bg-accent-hover disabled:opacity-40"
          >
            Connect
          </button>
        </div>

        <div className="flex-1 overflow-auto p-3 font-mono text-xs space-y-2">
          {log.length === 0 && <div className="text-fg-faint">Connect, then try: PING · KEYS * · GET key · HGETALL hash</div>}
          {log.map((e, i) => (
            <div key={i}>
              <div className="text-accent">&gt; {e.q}</div>
              <pre className={`m-0 whitespace-pre-wrap ${e.err ? "text-danger" : "text-fg"}`}>{e.r}</pre>
            </div>
          ))}
        </div>

        <div className="px-3 py-2 border-t border-border flex items-center gap-2">
          <span className="text-fg-faint font-mono text-xs">&gt;</span>
          <input
            value={cmd}
            onChange={(e) => setCmd(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") void run(); }}
            placeholder="GET user:1"
            disabled={!connected || busy}
            className="flex-1 bg-bg-elevated border border-border rounded px-2 py-1 text-xs font-mono outline-none focus:border-accent disabled:opacity-50"
          />
          <button
            onClick={run}
            disabled={!connected || busy}
            className="text-xs px-3 py-1 rounded bg-bg-elevated border border-border hover:bg-bg-hover disabled:opacity-40"
          >
            Run
          </button>
        </div>
      </div>
    </div>
  );
}
