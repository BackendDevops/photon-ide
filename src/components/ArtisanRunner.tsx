import { useEffect, useRef, useState } from "react";
import { api } from "../lib/api";

// "Run Artisan Command" — type a command (with completion from `artisan list`),
// run it, and see the output.
export default function ArtisanRunner({ onClose }: { onClose: () => void }) {
  const [cmds, setCmds] = useState<string[]>([]);
  const [input, setInput] = useState("");
  const [output, setOutput] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    api.artisanCommands().then(setCmds).catch(() => setCmds([]));
    requestAnimationFrame(() => inputRef.current?.focus());
  }, []);

  const run = async () => {
    if (!input.trim()) return;
    setBusy(true);
    setOutput(null);
    try {
      setOutput(await api.runArtisan(input.trim()));
    } catch (e) {
      setOutput(String(e));
    } finally {
      setBusy(false);
    }
  };

  const matches = input
    ? cmds.filter((c) => c.includes(input.split(" ")[0])).slice(0, 8)
    : cmds.slice(0, 8);

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center pt-[14vh] bg-black/50 backdrop-blur-sm" onClick={onClose}>
      <div
        className="pop-in w-[640px] max-w-[92vw] surface-5 border border-line/60 rounded-xl overflow-hidden"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-4 py-2.5 border-b border-line text-sm text-fg-muted flex items-center gap-2">
          <span className="text-[#ff7a6e]">⚡</span> php artisan
        </div>
        <div className="p-3">
          <input
            ref={inputRef}
            value={input}
            list="artisan-cmds"
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") run(); if (e.key === "Escape") onClose(); }}
            placeholder="e.g. make:controller PostController  ·  migrate  ·  route:list"
            className="w-full bg-bg-elevated border border-border rounded-md px-2.5 py-2 text-sm font-mono outline-none focus:border-accent"
          />
          <datalist id="artisan-cmds">
            {cmds.map((c) => (
              <option key={c} value={c} />
            ))}
          </datalist>
          {!input && matches.length > 0 && (
            <div className="mt-2 flex flex-wrap gap-1.5">
              {matches.map((c) => (
                <button
                  key={c}
                  onClick={() => setInput(c + " ")}
                  className="text-2xs px-2 py-0.5 rounded bg-white/[0.05] text-fg-muted hover:text-fg hover:bg-white/[0.1]"
                >
                  {c}
                </button>
              ))}
            </div>
          )}
          {output !== null && (
            <pre className="mt-3 max-h-[40vh] overflow-auto text-xs font-mono bg-bg rounded-md border border-border p-2.5 text-fg-muted whitespace-pre-wrap">
              {output || "(no output)"}
            </pre>
          )}
        </div>
        <div className="flex items-center justify-end gap-2 px-4 py-2.5 border-t border-line">
          <button onClick={onClose} className="btn-ghost text-xs">Close</button>
          <button onClick={run} disabled={busy || !input.trim()} className="btn-primary text-xs">
            {busy ? "Running…" : "Run"}
          </button>
        </div>
      </div>
    </div>
  );
}
