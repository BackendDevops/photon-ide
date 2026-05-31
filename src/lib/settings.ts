import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

// App settings persisted in the WebView's localStorage (this is a real desktop
// app, not a claude.ai artifact). A custom event broadcasts live changes.

export interface Settings {
  /** App-wide UI scale (affects everything except the editor text). */
  uiScale: number;
  editorFontSize: number;
  tabSize: number;
  wordWrap: boolean;
  minimap: boolean;
  stickyScroll: boolean;
  ligatures: boolean;
  terminalFontSize: number;
  keymap: "photon" | "phpstorm" | "vscode";
  /** Native Vim mode (runs on the editor, no extension). */
  vimMode: boolean;
  /** Auto-save the active file after a short idle (ms); 0 = off. */
  autoSave: boolean;
  autoSaveDelayMs: number;
  /** AI provider (OpenAI-compatible; BYO-key). */
  aiBaseUrl: string;
  aiModel: string;
  aiApiKey: string;
}

export const DEFAULT_SETTINGS: Settings = {
  uiScale: 1.0,
  editorFontSize: 16,
  tabSize: 4,
  wordWrap: false,
  minimap: true,
  stickyScroll: true,
  ligatures: true,
  terminalFontSize: 15,
  keymap: "phpstorm",
  vimMode: false,
  autoSave: true,
  autoSaveDelayMs: 1000,
  aiBaseUrl: "https://api.openai.com/v1",
  aiModel: "gpt-4o-mini",
  aiApiKey: "",
};

const KEY = "photon.settings";
const EVENT = "photon-settings-changed";

export function loadSettings(): Settings {
  try {
    return { ...DEFAULT_SETTINGS, ...JSON.parse(localStorage.getItem(KEY) || "{}") };
  } catch {
    return DEFAULT_SETTINGS;
  }
}

export function saveSettings(s: Settings) {
  const json = JSON.stringify(s);
  // Fast in-session cache + live broadcast…
  try {
    localStorage.setItem(KEY, json);
  } catch {
    /* WebView may sandbox localStorage — disk is authoritative below */
  }
  window.dispatchEvent(new CustomEvent<Settings>(EVENT, { detail: s }));
  // …and durable persistence to the OS app-config dir (survives restarts).
  invoke("settings_save", { json }).catch(() => {});
}

/// Hydrate settings from disk at startup and broadcast them. Call once on mount.
export async function hydrateSettings(): Promise<void> {
  try {
    const json = await invoke<string>("settings_load");
    if (json && json.trim()) {
      const parsed = { ...DEFAULT_SETTINGS, ...JSON.parse(json) } as Settings;
      localStorage.setItem(KEY, JSON.stringify(parsed));
      window.dispatchEvent(new CustomEvent<Settings>(EVENT, { detail: parsed }));
    }
  } catch {
    /* not in Tauri / no file yet */
  }
}

export function useSettings(): Settings {
  const [s, setS] = useState<Settings>(loadSettings);
  useEffect(() => {
    const handler = (e: Event) => setS((e as CustomEvent<Settings>).detail);
    window.addEventListener(EVENT, handler);
    // Pull persisted settings from disk on first mount.
    void hydrateSettings();
    return () => window.removeEventListener(EVENT, handler);
  }, []);
  return s;
}
