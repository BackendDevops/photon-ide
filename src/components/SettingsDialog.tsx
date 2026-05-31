import { useState } from "react";
import { DEFAULT_SETTINGS, saveSettings, type Settings } from "../lib/settings";

export default function SettingsDialog({
  current,
  onClose,
}: {
  current: Settings;
  onClose: () => void;
}) {
  const [s, setS] = useState<Settings>(current);

  const apply = (next: Settings) => {
    setS(next);
    saveSettings(next); // live apply
  };

  const Row = ({ label, children }: { label: string; children: React.ReactNode }) => (
    <div className="flex items-center justify-between py-2 border-b border-border/40">
      <span className="text-fg-muted text-sm">{label}</span>
      {children}
    </div>
  );

  const num =
    "w-16 bg-bg-elevated border border-border rounded px-2 py-1 text-sm text-fg outline-none focus:border-accent";

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
      onClick={onClose}
    >
      <div
        className="pop-in w-[520px] max-w-[92vw] bg-bg-panel border border-border rounded-xl shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="px-4 py-3 border-b border-border text-sm text-fg-muted">
          Settings
        </div>
        <div className="px-4 py-2">
          <div className="panel-title px-0">Appearance</div>
          <Row label="UI scale">
            <select
              className="bg-bg-elevated border border-border rounded px-2 py-1 text-sm"
              value={s.uiScale}
              onChange={(e) => apply({ ...s, uiScale: Number(e.target.value) })}
            >
              <option value={0.9}>90% (compact)</option>
              <option value={1.0}>100%</option>
              <option value={1.1}>110%</option>
              <option value={1.25}>125% (large)</option>
              <option value={1.4}>140% (XL)</option>
            </select>
          </Row>

          <div className="panel-title px-0 mt-2">Editor</div>
          <Row label="Font size">
            <input
              className={num}
              type="number"
              value={s.editorFontSize}
              onChange={(e) => apply({ ...s, editorFontSize: Number(e.target.value) })}
            />
          </Row>
          <Row label="Tab size">
            <input
              className={num}
              type="number"
              value={s.tabSize}
              onChange={(e) => apply({ ...s, tabSize: Number(e.target.value) })}
            />
          </Row>
          <Row label="Auto-save">
            <input type="checkbox" checked={s.autoSave} onChange={(e) => apply({ ...s, autoSave: e.target.checked })} />
          </Row>
          {s.autoSave && (
            <Row label="Auto-save delay (ms)">
              <input
                className={num}
                type="number"
                value={s.autoSaveDelayMs}
                onChange={(e) => apply({ ...s, autoSaveDelayMs: Math.max(200, Number(e.target.value)) })}
              />
            </Row>
          )}
          <Row label="Word wrap">
            <input type="checkbox" checked={s.wordWrap} onChange={(e) => apply({ ...s, wordWrap: e.target.checked })} />
          </Row>
          <Row label="Minimap">
            <input type="checkbox" checked={s.minimap} onChange={(e) => apply({ ...s, minimap: e.target.checked })} />
          </Row>
          <Row label="Sticky scroll">
            <input type="checkbox" checked={s.stickyScroll} onChange={(e) => apply({ ...s, stickyScroll: e.target.checked })} />
          </Row>
          <Row label="Font ligatures">
            <input type="checkbox" checked={s.ligatures} onChange={(e) => apply({ ...s, ligatures: e.target.checked })} />
          </Row>

          <div className="panel-title px-0 mt-2">Terminal</div>
          <Row label="Font size">
            <input
              className={num}
              type="number"
              value={s.terminalFontSize}
              onChange={(e) => apply({ ...s, terminalFontSize: Number(e.target.value) })}
            />
          </Row>

          <div className="panel-title px-0 mt-2">AI (BYO-key)</div>
          <Row label="Base URL">
            <input
              className="w-56 bg-bg-elevated border border-border rounded px-2 py-1 text-xs font-mono outline-none focus:border-accent"
              value={s.aiBaseUrl}
              onChange={(e) => apply({ ...s, aiBaseUrl: e.target.value })}
            />
          </Row>
          <Row label="Model">
            <input
              className="w-56 bg-bg-elevated border border-border rounded px-2 py-1 text-sm outline-none focus:border-accent"
              value={s.aiModel}
              onChange={(e) => apply({ ...s, aiModel: e.target.value })}
            />
          </Row>
          <Row label="API key">
            <input
              type="password"
              placeholder="sk-…  (stored locally)"
              className="w-56 bg-bg-elevated border border-border rounded px-2 py-1 text-sm outline-none focus:border-accent"
              value={s.aiApiKey}
              onChange={(e) => apply({ ...s, aiApiKey: e.target.value })}
            />
          </Row>
          <div className="text-fg-faint text-2xs px-1 -mt-1">
            OpenAI-compatible. For local models use{" "}
            <code>http://localhost:11434/v1</code> (Ollama).
          </div>

          <div className="panel-title px-0 mt-2">Keymap</div>
          <Row label="Preset">
            <select
              className="bg-bg-elevated border border-border rounded px-2 py-1 text-sm"
              value={s.keymap}
              onChange={(e) => apply({ ...s, keymap: e.target.value as Settings["keymap"] })}
            >
              <option value="phpstorm">PhpStorm</option>
              <option value="vscode">VS Code</option>
              <option value="photon">Photon</option>
            </select>
          </Row>
          <Row label="Vim mode">
            <input type="checkbox" checked={s.vimMode} onChange={(e) => apply({ ...s, vimMode: e.target.checked })} />
          </Row>
        </div>
        <div className="flex items-center justify-between px-4 py-3 border-t border-border">
          <button
            onClick={() => apply(DEFAULT_SETTINGS)}
            className="text-xs text-fg-faint hover:text-fg"
          >
            Reset to defaults
          </button>
          <button
            onClick={onClose}
            className="text-sm px-3 py-1.5 rounded bg-accent text-white hover:bg-accent-hover"
          >
            Done
          </button>
        </div>
      </div>
    </div>
  );
}
