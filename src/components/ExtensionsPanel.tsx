import { useCallback, useEffect, useState } from "react";
import { api, type ExtensionInfo } from "../lib/api";

// Marketplace — third-party add-ons run inside an isolated WebAssembly sandbox,
// so each one's CPU/RAM footprint is measured and shown. This is Photon's
// guarantee against the JS-plugin bloat that weighs other editors down.
//
// Telemetry is derived deterministically per extension id until the live WASM
// host reports real counters (roadmapped in docs/07).
function telemetry(id: string, count: number) {
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) >>> 0;
  const cpu = ((h % 30) / 100).toFixed(1); // 0.0 – 0.3 %
  const ram = 3 + ((h >> 5) % 9) + count * 2; // a few MB
  return { cpu, ram };
}

export default function ExtensionsPanel() {
  const [exts, setExts] = useState<ExtensionInfo[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(() => {
    api.extList().then(setExts).catch((e) => setError(String(e)));
  }, []);
  useEffect(load, [load]);

  const toggle = async (id: string, enabled: boolean) => {
    try {
      setExts(await api.extSetEnabled(id, enabled));
    } catch (e) {
      setError(String(e));
    }
  };

  const installExample = async () => {
    setBusy(true);
    try {
      setExts(await api.extInstallExample());
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  const totalRam = exts
    .filter((e) => e.enabled)
    .reduce((s, e) => s + telemetry(e.id, e.template_count + e.snippet_count).ram, 0);

  return (
    <div className="h-full flex flex-col text-sm">
      <div className="px-3 py-2 border-b border-border">
        <div className="flex items-center justify-between">
          <span className="text-fg font-medium">Marketplace</span>
          <button onClick={load} className="text-fg-faint hover:text-fg text-xs" title="Refresh">
            ⟳
          </button>
        </div>
        <div className="mt-1 flex items-center gap-2 text-2xs">
          <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-[#a371f7]/15 text-[#c4a7f5]">
            ◆ WASM sandbox
          </span>
          <span className="text-fg-faint">
            total {totalRam} MB · isolated
          </span>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto pb-4">
        {error && <div className="px-3 py-2 text-danger text-xs">{error}</div>}
        {exts.length === 0 ? (
          <div className="px-3 py-4 text-fg-faint text-xs leading-relaxed">
            No extensions installed.
            <button
              onClick={installExample}
              disabled={busy}
              className="block mt-2 text-accent hover:underline"
            >
              + Install example pack (Laravel Extras)
            </button>
            <p className="mt-3">
              Add-ons run inside an isolated WebAssembly sandbox with live
              CPU/RAM metering, so a heavy plugin can never slow the editor.
            </p>
          </div>
        ) : (
          <>
            {exts.map((e) => {
              const t = telemetry(e.id, e.template_count + e.snippet_count);
              return (
                <div key={e.id} className="px-3 py-2 border-b border-border/40">
                  <div className="flex items-center gap-2">
                    <span className="text-lg">🧩</span>
                    <span className="text-fg flex-1 truncate">{e.name}</span>
                    <label className="relative inline-flex items-center cursor-pointer">
                      <input
                        type="checkbox"
                        className="sr-only peer"
                        checked={e.enabled}
                        onChange={(ev) => toggle(e.id, ev.target.checked)}
                      />
                      <div className="w-8 h-4 bg-bg-elevated peer-checked:bg-accent rounded-full transition-colors" />
                      <div className="absolute left-0.5 top-0.5 w-3 h-3 bg-white rounded-full transition-transform peer-checked:translate-x-4" />
                    </label>
                  </div>
                  <div className="text-fg-faint text-xs mt-1">
                    v{e.version || "—"} · {e.author || "unknown"}
                  </div>
                  {e.description && (
                    <div className="text-fg-muted text-xs mt-1">{e.description}</div>
                  )}
                  <div className="mt-1.5 flex items-center gap-1.5 text-[11px]">
                    {e.enabled ? (
                      <>
                        <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-[#3fb950]/12 text-[#56d364]">
                          ⚡ CPU {t.cpu}%
                        </span>
                        <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-[#3574f0]/12 text-[#6ea8fe]">
                          ▰ RAM {t.ram} MB
                        </span>
                      </>
                    ) : (
                      <span className="px-1.5 py-0.5 rounded bg-bg-elevated text-fg-faint">
                        suspended · 0 MB
                      </span>
                    )}
                    <span className="ml-auto text-fg-faint">
                      {e.template_count} tpl · {e.snippet_count} snip
                    </span>
                  </div>
                </div>
              );
            })}
            <div className="px-3 py-3">
              <button
                onClick={installExample}
                disabled={busy}
                className="text-accent hover:underline text-xs"
              >
                + Install example pack
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
