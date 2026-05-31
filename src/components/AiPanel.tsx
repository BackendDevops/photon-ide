import { useEffect, useRef, useState } from "react";
import { api } from "../lib/api";
import { useSettings, saveSettings } from "../lib/settings";

interface Msg {
  role: "user" | "assistant" | "system";
  content: string;
}

// Dedicated AI workspace (docs/10): a project-aware chat. Context is the active
// file + project facts, sent as a system message so answers are grounded.
export default function AiPanel({
  getContext,
  onOpenSettings,
  pending,
}: {
  getContext: () => string;
  onOpenSettings: () => void;
  pending?: { text: string; nonce: number } | null;
}) {
  const settings = useSettings();
  const [messages, setMessages] = useState<Msg[]>([]);
  const [input, setInput] = useState("");
  const [busy, setBusy] = useState(false);
  const [useContext, setUseContext] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: scrollRef.current.scrollHeight });
  }, [messages, busy]);

  // An external action (editor "Explain"/"Generate test") sends a prompt.
  useEffect(() => {
    if (pending?.text) void send(pending.text);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [pending?.nonce]);

  const send = async (explicit?: string) => {
    const text = (explicit ?? input).trim();
    if (!text || busy) return;
    setError(null);
    if (!settings.aiModel || !settings.aiBaseUrl) {
      setError("Configure the AI provider in Settings → AI.");
      return;
    }
    const next = [...messages, { role: "user" as const, content: text }];
    setMessages(next);
    if (!explicit) setInput("");
    setBusy(true);
    try {
      const reply = await api.aiChat(
        settings.aiBaseUrl,
        settings.aiApiKey,
        settings.aiModel,
        next.map((m) => ({ role: m.role, content: m.content })),
        useContext ? getContext() : undefined
      );
      setMessages((m) => [...m, { role: "assistant", content: reply }]);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="h-full flex flex-col text-sm">
      <div className="flex items-center justify-between px-3 py-2 border-b border-line">
        <span className="flex items-center gap-1.5 text-fg">
          <span className="text-ai">✦</span> AI Assistant
        </span>
        <div className="flex items-center gap-2">
          {(() => {
            const offline = /localhost|127\.0\.0\.1|11434/.test(settings.aiBaseUrl);
            return (
              <button
                onClick={() =>
                  saveSettings(
                    offline
                      ? { ...settings, aiBaseUrl: "https://api.openai.com/v1", aiModel: "gpt-4o-mini" }
                      : { ...settings, aiBaseUrl: "http://localhost:11434/v1", aiModel: "llama3.1" }
                  )
                }
                title={offline ? "Local LLM — code never leaves your machine" : "Switch to Offline-First (Ollama)"}
                className={`text-2xs px-1.5 py-0.5 rounded-full border ${offline ? "bg-success/15 text-success border-success/40" : "border-border text-fg-faint hover:text-fg"}`}
              >
                {offline ? "● Offline" : "Offline?"}
              </button>
            );
          })()}
          <span className="text-fg-faint text-2xs">{settings.aiModel || "no model"}</span>
          <button onClick={onOpenSettings} className="text-fg-faint hover:text-fg" title="AI settings">
            ⚙
          </button>
          {messages.length > 0 && (
            <button onClick={() => setMessages([])} className="text-fg-faint hover:text-fg text-2xs">
              clear
            </button>
          )}
        </div>
      </div>

      <div ref={scrollRef} className="flex-1 overflow-y-auto p-3 space-y-3">
        {messages.length === 0 && (
          <div className="text-fg-faint text-xs leading-relaxed">
            Ask about your code — the active file and project facts are sent as
            context. BYO-key, OpenAI-compatible (incl. local Ollama).
          </div>
        )}
        {messages.map((m, i) => (
          <div
            key={i}
            className={`rounded-lg px-3 py-2 ${
              m.role === "user"
                ? "bg-accent/15 ml-6"
                : "bg-surface-3 mr-6"
            }`}
          >
            <div className="text-2xs uppercase tracking-wider text-fg-faint mb-1">
              {m.role === "user" ? "You" : "Photon AI"}
            </div>
            <div className="whitespace-pre-wrap text-fg-muted leading-relaxed text-sm">
              {m.content}
            </div>
          </div>
        ))}
        {busy && <div className="text-fg-faint text-xs animate-pulse2">Photon is thinking…</div>}
        {error && <div className="text-danger text-xs">{error}</div>}
      </div>

      <div className="border-t border-line p-2">
        <label className="flex items-center gap-1.5 text-2xs text-fg-faint mb-1.5">
          <input type="checkbox" checked={useContext} onChange={(e) => setUseContext(e.target.checked)} />
          Send active file + project as context
        </label>
        <div className="relative">
          <textarea
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
                e.preventDefault();
                void send();
              }
            }}
            placeholder="Ask Photon…  (⌘/Ctrl+Enter to send)"
            className="w-full h-20 bg-bg-elevated border border-border rounded-md px-2.5 py-2 text-sm outline-none focus:border-accent resize-none"
          />
          <button
            onClick={() => void send()}
            disabled={busy || !input.trim()}
            className="absolute bottom-2 right-2 btn-primary !py-1 !px-2.5 text-xs"
          >
            Send
          </button>
        </div>
      </div>
    </div>
  );
}
