import { DiffEditor, type DiffOnMount } from "@monaco-editor/react";
import { monacoLang } from "../lib/api";

// Side-by-side diff (JetBrains-style) via Monaco's DiffEditor.
export default function DiffViewer({
  original,
  modified,
  file,
}: {
  original: string;
  modified: string;
  file: string;
}) {
  const lang = monacoLang(
    file.endsWith(".blade.php") ? "blade" : file.split(".").pop() || "other"
  );

  const onMount: DiffOnMount = (_editor, monaco) => {
    // Define the theme here too — the diff can open before any code editor has,
    // so we can't rely on EditorPane having registered it (avoids a white bg).
    monaco.editor.defineTheme("photon-dark", {
      base: "vs-dark",
      inherit: true,
      rules: [
        { token: "comment", foreground: "6e7681", fontStyle: "italic" },
        { token: "keyword", foreground: "c678dd" },
        { token: "string", foreground: "98c379" },
      ],
      colors: {
        "editor.background": "#0b0d11",
        "editor.foreground": "#e4e8ef",
        "diffEditor.insertedTextBackground": "#3fd07e22",
        "diffEditor.removedTextBackground": "#ff5d5d22",
        "diffEditor.insertedLineBackground": "#3fd07e14",
        "diffEditor.removedLineBackground": "#ff5d5d14",
        "editorLineNumber.foreground": "#3a414c",
        "editorGutter.background": "#0b0d11",
      },
    });
    monaco.editor.setTheme("photon-dark");
  };

  return (
    <DiffEditor
      className="flex-1"
      theme="photon-dark"
      original={original}
      modified={modified}
      language={lang}
      onMount={onMount}
      options={{
        renderSideBySide: true,
        readOnly: true,
        originalEditable: false,
        fontSize: 13,
        fontFamily: "JetBrains Mono, SFMono-Regular, Menlo, monospace",
        minimap: { enabled: false },
        scrollBeyondLastLine: false,
        renderOverviewRuler: true,
        diffWordWrap: "off",
        lineNumbers: "on",
      }}
    />
  );
}
