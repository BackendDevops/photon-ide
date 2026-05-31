// Lightweight native Vim mode for Monaco — no extension, runs directly on the
// editor instance. Supports the everyday motions/operators most editing needs:
// h j k l, w b e, 0 ^ $, gg G, i a I A o O, x, dd, yy, p P, u, Ctrl-r, visual
// (v) + d/y, and a minimal command line (:w :q :wq). Surfaces the current mode
// so the status bar can render -- NORMAL -- / -- INSERT -- etc.

import type * as Monaco from "monaco-editor";

export type VimMode = "normal" | "insert" | "visual" | "command";

type Ed = Monaco.editor.IStandaloneCodeEditor;

interface Opts {
  onMode: (mode: VimMode, command?: string) => void;
  onSave?: () => void;
  onClose?: () => void;
}

export function attachVim(editor: Ed, monaco: typeof Monaco, opts: Opts) {
  let mode: VimMode = "normal";
  let pending = ""; // multi-key prefix (g, d, y)
  let cmd = ""; // command-line buffer
  let register = "";
  let registerLinewise = false;
  let visualAnchor: Monaco.Position | null = null;

  const setMode = (m: VimMode) => {
    mode = m;
    editor.updateOptions({
      cursorStyle: m === "insert" ? "line" : "block",
    });
    opts.onMode(m, m === "command" ? ":" + cmd : undefined);
  };

  const fire = (id: string) => editor.trigger("vim", id, null);
  const pos = () => editor.getPosition()!;
  const model = () => editor.getModel()!;

  const enterInsert = () => setMode("insert");

  const deleteLine = () => {
    const m = model();
    const ln = pos().lineNumber;
    const last = m.getLineCount();
    register = m.getLineContent(ln) + "\n";
    registerLinewise = true;
    const range =
      ln < last
        ? new monaco.Range(ln, 1, ln + 1, 1)
        : new monaco.Range(ln - 1 >= 1 ? ln - 1 : ln, ln - 1 >= 1 ? m.getLineMaxColumn(ln - 1) : 1, ln, m.getLineMaxColumn(ln));
    editor.executeEdits("vim", [{ range, text: "" }]);
  };

  const yankLine = () => {
    register = model().getLineContent(pos().lineNumber) + "\n";
    registerLinewise = true;
  };

  const paste = (after: boolean) => {
    const p = pos();
    if (registerLinewise) {
      const ln = p.lineNumber;
      const insertLn = after ? ln + 1 : ln;
      const col = 1;
      const text = register;
      editor.executeEdits("vim", [
        { range: new monaco.Range(insertLn, col, insertLn, col), text },
      ]);
      editor.setPosition({ lineNumber: insertLn, column: 1 });
    } else {
      const col = after ? p.column + 1 : p.column;
      editor.executeEdits("vim", [
        { range: new monaco.Range(p.lineNumber, col, p.lineNumber, col), text: register },
      ]);
    }
  };

  const extendVisual = () => {
    if (!visualAnchor) return;
    const p = pos();
    editor.setSelection(
      new monaco.Range(visualAnchor.lineNumber, visualAnchor.column, p.lineNumber, p.column)
    );
  };

  const runCommand = () => {
    const c = cmd;
    cmd = "";
    if (/w/.test(c)) opts.onSave?.();
    if (/q/.test(c)) opts.onClose?.();
    setMode("normal");
  };

  const disp = editor.onKeyDown((e) => {
    // Always let IDE shortcuts (Cmd/Ctrl) through, except Ctrl-r redo in normal.
    const key = e.browserEvent.key;

    if (mode === "command") {
      e.preventDefault();
      e.stopPropagation();
      if (key === "Enter") runCommand();
      else if (key === "Escape") {
        cmd = "";
        setMode("normal");
      } else if (key === "Backspace") {
        cmd = cmd.slice(0, -1);
        opts.onMode("command", ":" + cmd);
      } else if (key.length === 1) {
        cmd += key;
        opts.onMode("command", ":" + cmd);
      }
      return;
    }

    if (mode === "insert") {
      if (key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        setMode("normal");
      }
      return; // everything else types normally
    }

    // normal / visual
    if (e.metaKey || e.altKey) return;
    if (e.ctrlKey) {
      if (key === "r") {
        e.preventDefault();
        e.stopPropagation();
        fire("redo");
      }
      return;
    }

    // From here we own the keystroke.
    const consume = () => {
      e.preventDefault();
      e.stopPropagation();
    };

    // pending operators (g, d, y)
    if (pending === "g") {
      pending = "";
      if (key === "g") {
        consume();
        editor.setPosition({ lineNumber: 1, column: 1 });
        editor.revealLine(1);
      }
      return;
    }
    if (pending === "d") {
      pending = "";
      if (key === "d") {
        consume();
        deleteLine();
      }
      return;
    }
    if (pending === "y") {
      pending = "";
      if (key === "y") {
        consume();
        yankLine();
      }
      return;
    }

    switch (key) {
      case "h": consume(); fire("cursorLeft"); if (mode === "visual") extendVisual(); break;
      case "l": consume(); fire("cursorRight"); if (mode === "visual") extendVisual(); break;
      case "j": consume(); fire("cursorDown"); if (mode === "visual") extendVisual(); break;
      case "k": consume(); fire("cursorUp"); if (mode === "visual") extendVisual(); break;
      case "w": consume(); fire("cursorWordStartRight"); if (mode === "visual") extendVisual(); break;
      case "b": consume(); fire("cursorWordStartLeft"); if (mode === "visual") extendVisual(); break;
      case "e": consume(); fire("cursorWordEndRight"); if (mode === "visual") extendVisual(); break;
      case "0": consume(); fire("cursorHome"); if (mode === "visual") extendVisual(); break;
      case "$": consume(); fire("cursorEnd"); if (mode === "visual") extendVisual(); break;
      case "^": consume(); fire("cursorHome"); if (mode === "visual") extendVisual(); break;
      case "G": consume(); { const last = model().getLineCount(); editor.setPosition({ lineNumber: last, column: 1 }); editor.revealLine(last); } break;
      case "g": consume(); pending = "g"; break;
      case "i": consume(); enterInsert(); break;
      case "a": consume(); fire("cursorRight"); enterInsert(); break;
      case "I": consume(); fire("cursorHome"); enterInsert(); break;
      case "A": consume(); fire("cursorEnd"); enterInsert(); break;
      case "o": consume(); fire("editor.action.insertLineAfter"); enterInsert(); break;
      case "O": consume(); fire("editor.action.insertLineBefore"); enterInsert(); break;
      case "x": consume(); fire("deleteRight"); break;
      case "d": consume(); if (mode === "visual") { fire("editor.action.clipboardCutAction"); setMode("normal"); } else pending = "d"; break;
      case "y": consume(); if (mode === "visual") { const sel = editor.getSelection(); if (sel) register = model().getValueInRange(sel); registerLinewise = false; setMode("normal"); } else pending = "y"; break;
      case "p": consume(); paste(true); break;
      case "P": consume(); paste(false); break;
      case "u": consume(); fire("undo"); break;
      case "v": consume(); if (mode === "visual") { setMode("normal"); editor.setPosition(pos()); } else { visualAnchor = pos(); setMode("visual"); } break;
      case ":": consume(); cmd = ""; setMode("command"); break;
      case "Escape": consume(); if (mode === "visual") { setMode("normal"); editor.setPosition(pos()); } break;
      default:
        // swallow other printable keys so they don't type in normal mode
        if (key.length === 1) consume();
        break;
    }
  });

  setMode("normal");

  return {
    dispose() {
      disp.dispose();
      editor.updateOptions({ cursorStyle: "line" });
    },
  };
}
