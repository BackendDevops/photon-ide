import { useState } from "react";
import { api } from "../lib/api";

// Community Hub — report bugs, request features, or browse GitHub Discussions
// without leaving the editor. Composes the issue inline, then hands off to the
// browser only on submit.
const REPO = "photon-ide/photon"; // configurable later via settings

type Tab = "bug" | "feature" | "discussions";

export default function CommunityHub({ onClose }: { onClose: () => void }) {
  const [tab, setTab] = useState<Tab>("bug");
  const [title, setTitle] = useState("");
  const [body, setBody] = useState("");

  const submit = () => {
    const labels = tab === "bug" ? "bug" : "enhancement";
    const url =
      `https://github.com/${REPO}/issues/new?` +
      `labels=${labels}` +
      `&title=${encodeURIComponent(title)}` +
      `&body=${encodeURIComponent(body)}`;
    void api.openExternal(url).catch(() => {});
    onClose();
  };

  const openDiscussions = () => {
    void api.openExternal(`https://github.com/${REPO}/discussions`).catch(() => {});
  };

  return (
    <div className="fixed inset-0 z-50 flex" onClick={onClose}>
      <div className="flex-1 bg-black/30" />
      <div
        className="w-[420px] max-w-[92vw] h-full bg-bg-panel border-l border-border shadow-2xl flex flex-col slide-in-right"
        onClick={(e) => e.stopPropagation()}
      >
        <div className="h-11 shrink-0 flex items-center justify-between px-3 border-b border-border">
          <span className="text-fg font-medium flex items-center gap-2">
            <span className="text-accent">✦</span> Community Hub
          </span>
          <button onClick={onClose} className="text-fg-faint hover:text-fg">✕</button>
        </div>

        <div className="flex items-center gap-1 px-2 py-2 border-b border-line text-xs">
          {([
            ["bug", "🐞 Report Bug"],
            ["feature", "✨ Request Feature"],
            ["discussions", "💬 Discussions"],
          ] as [Tab, string][]).map(([id, label]) => (
            <button
              key={id}
              onClick={() => setTab(id)}
              className={`px-2.5 py-1 rounded ${
                tab === id ? "bg-accent/15 text-accent" : "text-fg-faint hover:text-fg"
              }`}
            >
              {label}
            </button>
          ))}
        </div>

        {tab === "discussions" ? (
          <div className="flex-1 flex flex-col items-center justify-center gap-3 px-6 text-center">
            <p className="text-fg-muted text-sm leading-relaxed">
              Browse questions, ideas, and announcements from the Photon
              community on GitHub Discussions.
            </p>
            <button
              onClick={openDiscussions}
              className="px-3 py-1.5 rounded bg-accent text-white text-sm hover:bg-accent-hover"
            >
              Open Discussions ↗
            </button>
          </div>
        ) : (
          <div className="flex-1 flex flex-col gap-2 p-3 min-h-0">
            <input
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder={tab === "bug" ? "Bug summary" : "Feature title"}
              className="bg-bg-elevated border border-border rounded px-2.5 py-1.5 text-sm text-fg outline-none focus:border-accent"
            />
            <textarea
              value={body}
              onChange={(e) => setBody(e.target.value)}
              placeholder={
                tab === "bug"
                  ? "Steps to reproduce, expected vs. actual, version…"
                  : "What problem would this solve? How should it work?"
              }
              className="flex-1 min-h-0 resize-none bg-bg-elevated border border-border rounded px-2.5 py-2 text-sm text-fg-muted font-mono outline-none focus:border-accent"
            />
            <div className="flex items-center justify-between">
              <span className="text-2xs text-fg-faint">
                Opens a pre-filled GitHub issue ({REPO})
              </span>
              <button
                onClick={submit}
                disabled={!title.trim()}
                className="px-3 py-1.5 rounded bg-accent text-white text-sm hover:bg-accent-hover disabled:opacity-40"
              >
                Submit ↗
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
