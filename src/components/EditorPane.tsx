import Editor, { type OnMount } from "@monaco-editor/react";
import { useEffect, useRef, useState } from "react";
import { api, type CompletionData } from "../lib/api";
import { promoteConstructor } from "../lib/promote";
import { attachVim, type VimMode } from "../lib/vim";

// Shared across editor instances: completion data + one-time provider guard.
let providersRegistered = false;
let completionCache: CompletionData = {
  routes: [],
  configs: [],
  translations: [],
  classes: [],
  envs: [],
  middlewares: [],
  request_keys: [],
};
// Schema (table → columns) for SQL completion inside PHP string literals.
let schemaCache: [string, string[]][] = [];
// Blade view names for view()/@extends/@include completion.
let bladeViews: string[] = [];

const BLADE_DIRECTIVES = [
  "if", "elseif", "else", "endif", "unless", "endunless", "isset", "endisset",
  "empty", "endempty", "foreach", "endforeach", "forelse", "empty", "endforelse",
  "for", "endfor", "while", "endwhile", "switch", "case", "break", "default",
  "endswitch", "extends", "section", "endsection", "yield", "parent", "show",
  "include", "includeIf", "includeWhen", "each", "component", "endcomponent",
  "slot", "endslot", "props", "php", "endphp", "json", "csrf", "method", "error",
  "enderror", "auth", "endauth", "guest", "endguest", "can", "endcan", "cannot",
  "endcannot", "production", "env", "vite", "stack", "push", "endpush", "once",
  "endonce", "lang", "class", "checked", "selected", "disabled", "dd", "dump",
];

const SQL_KEYWORDS = [
  "SELECT", "FROM", "WHERE", "JOIN", "INNER JOIN", "LEFT JOIN", "RIGHT JOIN",
  "ON", "GROUP BY", "ORDER BY", "HAVING", "LIMIT", "OFFSET", "INSERT INTO",
  "VALUES", "UPDATE", "SET", "DELETE FROM", "AS", "AND", "OR", "NOT", "NULL",
  "LIKE", "IN", "BETWEEN", "IS", "EXISTS", "DISTINCT", "COUNT", "SUM", "AVG",
  "MAX", "MIN", "ASC", "DESC", "UNION", "CASE", "WHEN", "THEN", "ELSE", "END",
];

// Photon dark theme for Monaco, aligned with the Tailwind palette.
function defineTheme(monaco: typeof import("monaco-editor")) {
  monaco.editor.defineTheme("photon-dark", {
    base: "vs-dark",
    inherit: true,
    rules: [
      { token: "comment", foreground: "6e7681", fontStyle: "italic" },
      { token: "keyword", foreground: "c678dd" },
      { token: "string", foreground: "98c379" },
      { token: "number", foreground: "d29922" },
      { token: "type", foreground: "e5a13a" },
      { token: "function", foreground: "61afef" },
      { token: "variable", foreground: "e06c75" },
    ],
    colors: {
      "editor.background": "#0b0d11",
      "editor.foreground": "#e4e8ef",
      "editorLineNumber.foreground": "#3a414c",
      "editorLineNumber.activeForeground": "#9aa4b2",
      "editor.selectionBackground": "#2a4a86",
      "editor.lineHighlightBackground": "#12161d",
      "editorCursor.foreground": "#5b8cff",
      "editorIndentGuide.background1": "#1b2027",
      "editorIndentGuide.activeBackground1": "#323a47",
      "editorWidget.background": "#171b22",
      "editorWidget.border": "#252b35",
      "editorStickyScroll.background": "#0b0d11",
      "editorGutter.background": "#0b0d11",
    },
  });
}

import type { Settings } from "../lib/settings";

// Is `col` (1-based) inside a config/route/__/trans/view/env string key on `line`?
function keyContextAt(line: string, col: number): { kind: string; key: string } | null {
  const re = /(config|route|__|trans_choice|trans|@lang|lang|view|env)\(\s*(['"])([^'"]*)\2/g;
  let m: RegExpExecArray | null;
  const c = col - 1;
  while ((m = re.exec(line))) {
    const keyStart = m.index + m[0].length - m[3].length - 1;
    const keyEnd = keyStart + m[3].length;
    if (c >= keyStart && c <= keyEnd && m[3]) {
      const fn = m[1];
      const kind =
        fn === "config" || fn === "route" || fn === "view" || fn === "env"
          ? fn
          : "trans"; // __, trans, trans_choice, @lang, lang
      return { kind, key: m[3] };
    }
  }
  return null;
}

// Common Laravel validation rules (for `rules()` / validate() completion).
const VALIDATION_RULES = [
  "required", "nullable", "sometimes", "filled", "present", "prohibited",
  "string", "integer", "numeric", "boolean", "array", "json", "file", "image",
  "email", "url", "uuid", "ulid", "ip", "date", "date_format", "after", "before",
  "min", "max", "between", "size", "gt", "gte", "lt", "lte", "digits", "digits_between",
  "in", "not_in", "unique", "exists", "confirmed", "same", "different", "regex",
  "alpha", "alpha_num", "alpha_dash", "starts_with", "ends_with", "mimes", "mimetypes",
  "dimensions", "accepted", "declined", "enum", "password", "current_password",
  "required_if", "required_unless", "required_with", "required_without", "distinct",
];

// Curated PHP stdlib + Laravel global helpers for built-in completion (the
// PhpStorm-style "the language knows these" feel, without bundling full stubs).
const STDLIB_FUNCTIONS = [
  // strings
  "strlen", "strpos", "stripos", "strrpos", "str_contains", "str_starts_with",
  "str_ends_with", "str_replace", "str_ireplace", "substr", "substr_count", "trim",
  "ltrim", "rtrim", "strtolower", "strtoupper", "ucfirst", "ucwords", "lcfirst",
  "sprintf", "printf", "number_format", "str_repeat", "str_pad", "wordwrap", "nl2br",
  "explode", "implode", "preg_match", "preg_match_all", "preg_replace", "preg_split",
  "preg_quote", "htmlspecialchars", "htmlentities", "strip_tags", "addslashes",
  // arrays
  "count", "array_map", "array_filter", "array_reduce", "array_merge", "array_keys",
  "array_values", "array_key_exists", "array_search", "in_array", "array_column",
  "array_combine", "array_flip", "array_unique", "array_slice", "array_splice",
  "array_push", "array_pop", "array_shift", "array_unshift", "array_reverse",
  "array_fill", "array_diff", "array_intersect", "array_chunk", "array_pad",
  "sort", "rsort", "usort", "uasort", "uksort", "ksort", "asort", "arsort",
  "range", "compact", "extract", "list",
  // math
  "abs", "ceil", "floor", "round", "max", "min", "pow", "sqrt", "intdiv", "fmod",
  "rand", "mt_rand", "random_int", "intval", "floatval", "is_nan",
  // types / vars
  "is_array", "is_string", "is_int", "is_integer", "is_float", "is_bool", "is_null",
  "is_numeric", "is_callable", "is_object", "is_iterable", "isset", "empty", "unset",
  "gettype", "settype", "json_encode", "json_decode", "serialize", "unserialize",
  // misc
  "var_dump", "print_r", "var_export", "function_exists", "class_exists",
  "method_exists", "property_exists", "defined", "define", "constant", "call_user_func",
  "call_user_func_array", "func_get_args", "iterator_to_array", "date", "time",
  "strtotime", "microtime", "usleep", "sleep", "getenv", "putenv",
  // Laravel global helpers
  "collect", "now", "today", "app", "auth", "config", "route", "url", "asset",
  "view", "response", "redirect", "request", "session", "cache", "cookie", "old",
  "abort", "abort_if", "abort_unless", "back", "bcrypt", "encrypt", "decrypt",
  "event", "dispatch", "logger", "report", "rescue", "retry", "tap", "throw_if",
  "throw_unless", "value", "optional", "blank", "filled", "data_get", "data_set",
  "str", "fake", "trans", "trans_choice", "__", "validator", "policy", "gate",
];

// 1-based line to insert a new `use ...;` (after the last use / namespace / <?php).
function importInsertLine(text: string): number {
  const lines = text.split("\n");
  let line = 1;
  for (let i = 0; i < lines.length; i++) {
    const t = lines[i].trim();
    if (t.startsWith("use ")) line = i + 2;
    else if (t.startsWith("namespace ") && line === 1) line = i + 2;
    else if (t.startsWith("<?php") && line === 1) line = i + 2;
  }
  return line;
}

// The line of the `}` that closes the first `{` at/after `startLine` — used to
// delete a whole method body in a quick-fix. (String/comment braces are rare in
// signatures; good enough for member removal.)
function closingBraceLine(
  model: import("monaco-editor").editor.ITextModel,
  startLine: number,
): number {
  const total = model.getLineCount();
  let depth = 0;
  let started = false;
  for (let ln = startLine; ln <= total; ln++) {
    for (const ch of model.getLineContent(ln)) {
      if (ch === "{") {
        depth++;
        started = true;
      } else if (ch === "}") {
        depth--;
        if (started && depth === 0) return ln;
      }
    }
  }
  return startLine;
}

// The callee identifier of the innermost unclosed `(` before the cursor —
// drives named-argument completion. Returns null outside any call.
const ARG_CONTROL = new Set([
  "if", "elseif", "for", "foreach", "while", "switch", "match", "catch",
  "array", "list", "isset", "empty", "echo", "print", "return", "unset",
]);
function calleeBeforeArgs(back: string): string | null {
  let depth = 0;
  for (let i = back.length - 1; i >= 0; i--) {
    const ch = back[i];
    if (ch === ")") depth++;
    else if (ch === "(") {
      if (depth === 0) {
        let j = i - 1;
        while (j >= 0 && /\s/.test(back[j])) j--;
        const end = j + 1;
        while (j >= 0 && /[\w]/.test(back[j])) j--;
        const word = back.slice(j + 1, end);
        if (!word || ARG_CONTROL.has(word)) return null;
        return word;
      }
      depth--;
    }
  }
  return null;
}

// Laravel-aware completion + symbol hover, registered once globally.
function registerProviders(monaco: typeof import("monaco-editor")) {
  monaco.languages.registerCompletionItemProvider("php", {
    triggerCharacters: ["'", '"', ">", ":", "\\", ">"],
    async provideCompletionItems(model, position) {
      const line = model.getValueInRange({
        startLineNumber: position.lineNumber,
        startColumn: 1,
        endLineNumber: position.lineNumber,
        endColumn: position.column,
      });
      const word = model.getWordUntilPosition(position);
      const range = {
        startLineNumber: position.lineNumber,
        endLineNumber: position.lineNumber,
        startColumn: word.startColumn,
        endColumn: word.endColumn,
      };
      const K = monaco.languages.CompletionItemKind;
      const mk = (items: string[], kind: number, detail: string) =>
        items.map((label) => ({ label, kind, insertText: label, range, detail }));

      // Type-aware member completion after `->` or `::`. Resolve the chain ROOT
      // so `User::query()->where(...)->` still completes against the model.
      const upto = line.slice(0, position.column - 1);
      const head = upto.slice(0, upto.length - word.word.length);
      if (/(->|::)\s*$/.test(head)) {
        // Full trailing chain so the backend can walk types:
        // `$this->svc->find()->` → "$this->svc->find()".
        const chainM = head.match(
          /([$\\\w]+(?:(?:->|::)[\\\w]+(?:\([^()]*\))?)*)\s*(?:->|::)\s*$/
        );
        const receiver = chainM ? chainM[1] : "$this";
        const file = model.uri.path.replace(/^\//, "");
        const offset = model.getOffsetAt(position);
        try {
          const syms = await api.memberCompletions(file, offset, receiver);
          const kindOf = (k: string) =>
            k === "method"
              ? K.Method
              : k === "property"
              ? K.Field
              : k === "constant"
              ? K.Constant
              : K.Property;
          return {
            suggestions: syms.map((s) => ({
              label: s.name,
              kind: kindOf(s.kind),
              insertText: s.name,
              range,
              detail: s.container ? `${s.kind} · ${s.container}` : s.kind,
            })),
          };
        } catch {
          return { suggestions: [] };
        }
      }

      if (/route\(\s*['"][^'"]*$/.test(line))
        return { suggestions: mk(completionCache.routes, K.Value, "route") };
      if (/config\(\s*['"][^'"]*$/.test(line))
        return { suggestions: mk(completionCache.configs, K.Value, "config") };
      if (/(__|trans|trans_choice|@lang|lang)\(\s*['"][^'"]*$/.test(line))
        return { suggestions: mk(completionCache.translations, K.Value, "translation") };
      if (/env\(\s*['"][^'"]*$/.test(line))
        return { suggestions: mk(completionCache.envs, K.Value, "env") };
      if (/middleware\(\s*\[?\s*['"][^'"]*$/.test(line))
        return { suggestions: mk(completionCache.middlewares, K.Value, "middleware") };
      // Blade view names: view('…'), View::make('…'), @extends/@include('…').
      if (/(view|View::make|@extends|@include|@includeIf|@includeWhen|@each|@component)\s*\(?\s*['"][^'"]*$/.test(line))
        return { suggestions: mk(bladeViews, K.File, "view") };
      // Blade `<x-component>` — components live under `resources/views/components`.
      if (/<x-[\w.-]*$/.test(line)) {
        const comps = bladeViews
          .filter((v) => v.startsWith("components."))
          .map((v) => v.slice("components.".length).replace(/\./g, "."));
        return { suggestions: mk(comps, K.Class, "component") };
      }
      // Blade directives: `@if`, `@foreach`, `@section`, …
      if (/(^|[^\w])@\w*$/.test(line))
        return { suggestions: mk(BLADE_DIRECTIVES, K.Keyword, "directive") };
      // $request->input('…') / request('…') / ->validated('…') etc.
      if (/(->(?:input|get|string|integer|boolean|has|filled|validated|date|enum|collect|only|except)|request)\(\s*['"][^'"]*$/.test(line))
        return { suggestions: mk(completionCache.request_keys, K.Field, "input") };

      // Validation rules inside rules()/validate()/Validator::make (look back a
      // few lines for the validation context to avoid false positives).
      if (/['"][a-z_|:,\d\s]*$/.test(line)) {
        const back = model.getValueInRange({
          startLineNumber: Math.max(1, position.lineNumber - 15),
          startColumn: 1,
          endLineNumber: position.lineNumber,
          endColumn: position.column,
        });
        if (/function\s+rules\s*\(|->validate\s*\(|Validator::make\s*\(|protected\s+\$rules/.test(back)) {
          return { suggestions: mk(VALIDATION_RULES, K.EnumMember, "rule") };
        }
      }

      // Schema-aware SQL completion inside a PHP string literal.
      {
        const inString =
          (line.match(/"/g) || []).length % 2 === 1 || (line.match(/'/g) || []).length % 2 === 1;
        const looksSql =
          /\b(select|from|where|join|into|values|update|set|delete|group by|order by|having|limit|inner join|left join)\b/i.test(line);
        let sqlCall = false;
        if (inString && !looksSql) {
          const back = model.getValueInRange({
            startLineNumber: Math.max(1, position.lineNumber - 3),
            startColumn: 1,
            endLineNumber: position.lineNumber,
            endColumn: position.column,
          });
          sqlCall =
            /(DB::(select|statement|insert|update|delete|raw|unprepared)|->(raw|selectRaw|whereRaw|orderByRaw|havingRaw|fromRaw|groupByRaw))\s*\(\s*['"][^'"]*$/.test(back);
        }
        if (inString && (looksSql || sqlCall)) {
          const kw = SQL_KEYWORDS.map((k) => ({ label: k, kind: K.Keyword, insertText: k, range, detail: "sql" }));
          const tables = schemaCache.map(([t]) => ({ label: t, kind: K.Class, insertText: t, range, detail: "table" }));
          const cols = Array.from(new Set(schemaCache.flatMap(([, c]) => c))).map((c) => ({
            label: c,
            kind: K.Field,
            insertText: c,
            range,
            detail: "column",
          }));
          return { suggestions: [...tables, ...cols, ...kw] };
        }
      }

      // Attribute completion (PHP 8): after `#[`, offer attribute classes.
      if (/#\[\s*\\?[\w\\]*$/.test(line)) {
        return {
          suggestions: completionCache.classes.map((label) => ({
            label,
            kind: K.Class,
            insertText: label,
            range,
            detail: "attribute",
          })),
        };
      }

      // General context: PHP keywords + snippets + class names (Monaco filters
      // by the typed prefix, so `pub`→public, `fun`→function, etc.).
      const keywords = [
        "public", "private", "protected", "static", "abstract", "final", "readonly",
        "function", "class", "interface", "trait", "enum", "namespace", "use", "const",
        "return", "foreach", "for", "while", "if", "else", "elseif", "switch", "match",
        "new", "fn", "true", "false", "null", "array", "echo", "throw", "try", "catch",
        "finally", "extends", "implements", "instanceof", "yield", "default",
        // Modern type & language keywords (PHP 8.0 – 8.5).
        "never", "void", "mixed", "iterable", "object", "callable", "self", "parent",
        "clone", "declare", "as", "insteadof", "do", "global", "list", "and", "or",
        // Property hooks & asymmetric visibility (8.4) — completion awareness.
        "get", "set",
      ].map((k) => ({ label: k, kind: K.Keyword, insertText: k, range, detail: "keyword" }));

      const snip = monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet;
      const snippets = [
        { label: "function", body: "function ${1:name}(${2})\n{\n\t$0\n}" },
        { label: "__construct", body: "public function __construct(${1})\n{\n\t$0\n}" },
        { label: "foreach", body: "foreach (${1:\\$items} as ${2:\\$item}) {\n\t$0\n}" },
        { label: "fn", body: "fn(${1}) => ${2}" },
        { label: "if", body: "if (${1}) {\n\t$0\n}" },
      ].map((s) => ({
        label: s.label,
        kind: K.Snippet,
        insertText: s.body,
        insertTextRules: snip,
        range,
        detail: "snippet",
      }));

      const classes = completionCache.classes.map((label) => ({
        label,
        kind: K.Class,
        insertText: label,
        range,
        detail: "class",
      }));

      // Built-in PHP stdlib + Laravel global helpers (stub awareness).
      const stdlib = STDLIB_FUNCTIONS.map((label) => ({
        label,
        kind: K.Function,
        insertText: `${label}($0)`,
        insertTextRules: snip,
        range,
        detail: "built-in",
      }));

      // Named-argument completion (PHP 8.0+): inside a call, offer `param:`.
      let namedArgs: typeof keywords = [];
      const argBack = model.getValueInRange({
        startLineNumber: Math.max(1, position.lineNumber - 8),
        startColumn: 1,
        endLineNumber: position.lineNumber,
        endColumn: position.column,
      });
      const callee = calleeBeforeArgs(argBack);
      if (callee) {
        try {
          const ps = await api.callParams(callee);
          namedArgs = ps.map((p) => ({
            label: `${p}:`,
            kind: K.Field,
            insertText: `${p}: `,
            range,
            detail: "named arg",
          }));
        } catch {
          /* ignore */
        }
      }

      return { suggestions: [...namedArgs, ...keywords, ...snippets, ...classes, ...stdlib] };
    },
  });

  // Quick-fix: import an unresolved class (adds a `use ...;`).
  monaco.languages.registerCodeActionProvider("php", {
    async provideCodeActions(model, range, context) {
      const actions: import("monaco-editor").languages.CodeAction[] = [];
      const insertLine = importInsertLine(model.getValue());

      // PHP 8.3 #[Override] — offer on a method declaration inside a class that
      // extends/implements something, when the attribute isn't already present.
      {
        const ln = range.startLineNumber;
        const lineText = model.getLineContent(ln);
        const mdecl = lineText.match(/^(\s*)(?:(?:public|protected|private|final|abstract|static)\s+)*function\s+(\w+)\s*\(/);
        const src = model.getValue();
        const polymorphic = /\b(extends|implements)\b/.test(src);
        const prev = ln > 1 ? model.getLineContent(ln - 1).trim() : "";
        if (mdecl && polymorphic && !/#\[\s*Override\s*\]/.test(prev) && mdecl[2] !== "__construct") {
          const indent = mdecl[1];
          actions.push({
            title: "Add #[Override] attribute",
            kind: "quickfix",
            edit: {
              edits: [
                {
                  resource: model.uri,
                  versionId: model.getVersionId(),
                  textEdit: {
                    range: new monaco.Range(ln, 1, ln, 1),
                    text: `${indent}#[\\Override]\n`,
                  },
                },
              ],
            },
          });
        }
      }

      // PHP 8 constructor property promotion — when the cursor is on a
      // __construct that assigns its params to $this->x, offer to promote.
      {
        const ln = range.startLineNumber;
        if (/function\s+__construct\s*\(/.test(model.getLineContent(ln))) {
          try {
            const promoted = promoteConstructor(model.getValue(), ln);
            if (promoted) {
              actions.push({
                title: "Convert to constructor property promotion",
                kind: "refactor.rewrite",
                edit: {
                  edits: [
                    {
                      resource: model.uri,
                      versionId: model.getVersionId(),
                      textEdit: {
                        range: model.getFullModelRange(),
                        text: promoted,
                      },
                    },
                  ],
                },
              });
            }
          } catch {
            /* never block other quick-fixes */
          }
        }
      }
      for (const marker of context.markers) {
        // Remove unused / duplicate import.
        if (/Unused import|Duplicate import/.test(marker.message)) {
          const ln = marker.startLineNumber;
          actions.push({
            title: "Remove import",
            kind: "quickfix",
            diagnostics: [marker],
            isPreferred: true,
            edit: {
              edits: [
                {
                  resource: model.uri,
                  versionId: model.getVersionId(),
                  textEdit: {
                    range: new monaco.Range(ln, 1, ln + 1, 1),
                    text: "",
                  },
                },
              ],
            },
          });
          continue;
        }
        // Remove a leftover debug statement or an unreachable line.
        if (/Leftover debug statement|Unreachable code/.test(marker.message)) {
          const ln = marker.startLineNumber;
          actions.push({
            title: /debug/.test(marker.message) ? "Remove debug statement" : "Remove unreachable code",
            kind: "quickfix",
            diagnostics: [marker],
            isPreferred: true,
            edit: {
              edits: [
                {
                  resource: model.uri,
                  versionId: model.getVersionId(),
                  textEdit: { range: new monaco.Range(ln, 1, ln + 1, 1), text: "" },
                },
              ],
            },
          });
          continue;
        }
        // Remove an unused private property (single line) or method (brace-matched).
        const up = marker.message.match(/Unused private (method|property) '([^']+)'/);
        if (up) {
          const ln = marker.startLineNumber;
          const endLn = up[1] === "method" ? closingBraceLine(model, ln) : ln;
          actions.push({
            title: `Remove unused ${up[1]} '${up[2]}'`,
            kind: "quickfix",
            diagnostics: [marker],
            edit: {
              edits: [
                {
                  resource: model.uri,
                  versionId: model.getVersionId(),
                  textEdit: { range: new monaco.Range(ln, 1, endLn + 1, 1), text: "" },
                },
              ],
            },
          });
          continue;
        }
        // Add an inferred return type where one is missing.
        if (/Missing return type/.test(marker.message)) {
          const file = model.uri.path.replace(/^\//, "");
          try {
            const fix = await api.returnTypeFix(file, marker.startLineNumber);
            if (fix) {
              actions.push({
                title: `Add return type ': ${fix.ty}'`,
                kind: "quickfix",
                diagnostics: [marker],
                isPreferred: true,
                edit: {
                  edits: [
                    {
                      resource: model.uri,
                      versionId: model.getVersionId(),
                      textEdit: {
                        range: new monaco.Range(fix.line, fix.col, fix.line, fix.col),
                        text: `: ${fix.ty}`,
                      },
                    },
                  ],
                },
              });
            }
          } catch {
            /* ignore */
          }
          continue;
        }
        const mm = marker.message.match(/Class '([^']+)' is not imported/);
        if (!mm) continue;
        let fqns: string[] = [];
        try {
          const syms = await api.gotoSymbol(mm[1]);
          fqns = syms
            .filter((s) => s.fqn && ["class", "interface", "trait", "enum"].includes(s.kind))
            .map((s) => s.fqn as string)
            .slice(0, 6);
        } catch {
          /* ignore */
        }
        for (const fqn of fqns) {
          actions.push({
            title: `Import ${fqn}`,
            kind: "quickfix",
            diagnostics: [marker],
            isPreferred: fqns.length === 1,
            edit: {
              edits: [
                {
                  resource: model.uri,
                  versionId: model.getVersionId(),
                  textEdit: {
                    range: new monaco.Range(insertLine, 1, insertLine, 1),
                    text: `use ${fqn};\n`,
                  },
                },
              ],
            },
          });
        }
      }
      return { actions, dispose() {} };
    },
  });

  monaco.languages.registerHoverProvider("php", {
    async provideHover(model, position) {
      const w = model.getWordAtPosition(position);
      if (!w) return null;
      try {
        const doc = await api.symbolDoc(w.word);
        if (doc) {
          const parts: string[] = [];
          // Signature as a PHP code block (PhpStorm-style).
          parts.push("```php\n" + (doc.signature || `${doc.kind} ${doc.name}`) + "\n```");
          if (doc.doc) parts.push(doc.doc);
          if (doc.params.length) {
            const ps = doc.params
              .map(([ty, name]) => (ty ? `\`${ty}\` $${name}` : `$${name}`))
              .join(", ");
            parts.push(`**Parameters:** ${ps}`);
          }
          if (doc.return_type) parts.push(`**Returns:** \`${doc.return_type}\``);
          if (doc.source) parts.push(`_Source: ${doc.source}_`);
          return { contents: [{ value: parts.join("\n\n") }] };
        }
        // Fallback to the lightweight lookup.
        const syms = await api.gotoSymbol(w.word);
        if (!syms.length) return null;
        const s = syms[0];
        const value = [
          `**${s.name}**  \`${s.kind}\``,
          s.fqn ? "`" + s.fqn + "`" : "",
          `_${s.file}:${s.line}_`,
        ]
          .filter(Boolean)
          .join("\n\n");
        return { contents: [{ value }] };
      } catch {
        return null;
      }
    },
  });
}

export default function EditorPane({
  path,
  value,
  language,
  reveal,
  settings,
  lintKey,
  onChange,
  onSave,
  onRequestRename,
  onFindUsages,
  onExtractVariable,
  onExtractMethod,
  onInlineVariable,
  onSafeDelete,
  onChangeSignature,
  onMoveClass,
  onLocalHistory,
  onToggleBreakpoint,
  onConditionalBreakpoint,
  breakpointLines,
  onCmdClick,
  onResolveKey,
  onResolveBinding,
  onGotoDefinition,
  onGotoImplementation,
  onGotoType,
  onAiAsk,
  debugActive,
  debugLine,
  debugInline,
  testLines,
  onRunTest,
}: {
  path: string | null;
  value: string;
  language: string;
  reveal: { line: number; nonce: number } | null;
  settings: Settings;
  lintKey: number;
  onChange: (v: string) => void;
  onSave: () => void;
  onRequestRename: (word: string) => void;
  onFindUsages: (word: string) => void;
  onExtractVariable: (sel: { start: number; end: number; line: number }) => void;
  onExtractMethod: (sel: { start: number; end: number; line: number }) => void;
  onInlineVariable: (word: string) => void;
  onSafeDelete: (word: string) => void;
  onChangeSignature: (line: number, params: string) => void;
  onMoveClass: (word: string) => void;
  onLocalHistory: () => void;
  onToggleBreakpoint: (line: number) => void;
  onConditionalBreakpoint: (line: number) => void;
  breakpointLines: number[];
  onCmdClick: (info: {
    word: string;
    line: number;
    offset: number;
    x: number;
    y: number;
    chain: string | null;
  }) => void;
  onResolveKey: (kind: string, key: string) => void;
  onResolveBinding: (word: string) => void;
  onGotoDefinition: (word: string) => void;
  onGotoImplementation: (word: string) => void;
  onGotoType: (chain: string, offset: number) => void;
  onAiAsk: (prompt: string) => void;
  debugActive: boolean;
  debugLine: number | null;
  debugInline: string | null;
  testLines: number[];
  onRunTest: (line: number) => void;
}) {
  // Use distinct generic types for the editor + monaco namespace refs.
  const editorRef = useRef<Parameters<OnMount>[0] | null>(null);
  const monacoRef = useRef<Parameters<OnMount>[1] | null>(null);
  const [mounted, setMounted] = useState(false);
  const [vimState, setVimState] = useState<{ mode: VimMode; cmd: string } | null>(null);
  const onSaveRef = useRef(onSave);
  onSaveRef.current = onSave;

  const handleMount: OnMount = (editor, monaco) => {
    editorRef.current = editor;
    monacoRef.current = monaco;
    setMounted(true);
    defineTheme(monaco as unknown as typeof import("monaco-editor"));
    monaco.editor.setTheme("photon-dark");
    // Ctrl/Cmd+S saves through the Rust layer (which re-indexes the file).
    editor.addCommand(
      monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS,
      () => onSave()
    );

    const wordAtCursor = (): string => {
      const pos = editor.getPosition();
      const model = editor.getModel();
      if (!pos || !model) return "";
      return model.getWordAtPosition(pos)?.word ?? "";
    };

    // Alt+Enter → show contextual actions (quick-fixes / intentions), JetBrains-style.
    editor.addAction({
      id: "photon.intentions",
      label: "Photon: Show Context Actions",
      keybindings: [monaco.KeyMod.Alt | monaco.KeyCode.Enter],
      run: (ed) => {
        ed.trigger("photon", "editor.action.quickFix", null);
      },
    });

    // F2 → Rename symbol (Safe Rename across the project).
    editor.addAction({
      id: "photon.rename",
      label: "Photon: Rename Symbol",
      keybindings: [monaco.KeyCode.F2],
      run: () => {
        const w = wordAtCursor();
        if (w) onRequestRename(w);
      },
    });

    // F12 / Cmd+B → Go to Definition (resolves into vendor/framework too).
    editor.addAction({
      id: "photon.gotoDefinition",
      label: "Photon: Go to Definition",
      keybindings: [monaco.KeyCode.F12, monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyB],
      run: () => {
        const w = wordAtCursor();
        if (w) onGotoDefinition(w);
      },
    });

    // Cmd/Ctrl+Alt+B → Go to Implementation(s).
    editor.addAction({
      id: "photon.gotoImplementation",
      label: "Photon: Go to Implementation",
      keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyMod.Alt | monaco.KeyCode.KeyB],
      run: () => {
        const w = wordAtCursor();
        if (w) onGotoImplementation(w);
      },
    });

    // Shift+F12 → Find Usages.
    editor.addAction({
      id: "photon.findUsages",
      label: "Photon: Find Usages",
      keybindings: [monaco.KeyMod.Shift | monaco.KeyCode.F12],
      run: () => {
        const w = wordAtCursor();
        if (w) onFindUsages(w);
      },
    });

    // Ctrl/Cmd+Alt+V → Extract Variable (uses the current selection).
    editor.addAction({
      id: "photon.extractVariable",
      label: "Photon: Extract Variable",
      keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyMod.Alt | monaco.KeyCode.KeyV],
      contextMenuGroupId: "1_modification",
      run: () => {
        const sel = editor.getSelection();
        const model = editor.getModel();
        if (!sel || !model || sel.isEmpty()) return;
        onExtractVariable({
          start: model.getOffsetAt({ lineNumber: sel.startLineNumber, column: sel.startColumn }),
          end: model.getOffsetAt({ lineNumber: sel.endLineNumber, column: sel.endColumn }),
          line: sel.startLineNumber,
        });
      },
    });
    editor.addAction({
      id: "photon.extractMethod",
      label: "Photon: Extract Method",
      keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyMod.Alt | monaco.KeyCode.KeyM],
      contextMenuGroupId: "1_modification",
      run: () => {
        const sel = editor.getSelection();
        const model = editor.getModel();
        if (!sel || !model || sel.isEmpty()) return;
        onExtractMethod({
          start: model.getOffsetAt({ lineNumber: sel.startLineNumber, column: sel.startColumn }),
          end: model.getOffsetAt({ lineNumber: sel.endLineNumber, column: sel.endColumn }),
          line: sel.startLineNumber,
        });
      },
    });
    editor.addAction({
      id: "photon.inlineVariable",
      label: "Photon: Inline Variable",
      keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyMod.Alt | monaco.KeyCode.KeyN],
      contextMenuGroupId: "1_modification",
      run: () => {
        const w = wordAtCursor();
        if (w) onInlineVariable(w);
      },
    });
    editor.addAction({
      id: "photon.safeDelete",
      label: "Photon: Safe Delete",
      contextMenuGroupId: "1_modification",
      run: () => {
        const w = wordAtCursor();
        if (w) onSafeDelete(w);
      },
    });

    // Cmd/Ctrl+F6 → Change Signature (edit the parameter list).
    editor.addAction({
      id: "photon.changeSignature",
      label: "Photon: Change Signature",
      keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.F6],
      contextMenuGroupId: "1_modification",
      run: () => {
        const pos = editor.getPosition();
        const model = editor.getModel();
        if (!pos || !model) return;
        const text = model.getLineContent(pos.lineNumber);
        const open = text.indexOf("(");
        let params = "";
        if (open >= 0) {
          const close = text.indexOf(")", open);
          if (close > open) params = text.slice(open + 1, close);
        }
        onChangeSignature(pos.lineNumber, params);
      },
    });

    // Move Class / Change Namespace (context menu).
    editor.addAction({
      id: "photon.moveClass",
      label: "Photon: Move Class / Change Namespace",
      contextMenuGroupId: "1_modification",
      run: () => {
        const w = wordAtCursor();
        if (w) onMoveClass(w);
      },
    });

    // AI: explain the selection (or current line) and flag issues.
    editor.addAction({
      id: "photon.aiExplain",
      label: "Photon AI: Explain / Fix",
      contextMenuGroupId: "9_cutcopypaste",
      run: () => {
        const sel = editor.getSelection();
        const model = editor.getModel();
        if (!model) return;
        const snippet =
          sel && !sel.isEmpty()
            ? model.getValueInRange(sel)
            : model.getLineContent(editor.getPosition()?.lineNumber ?? 1);
        onAiAsk(
          "Explain this PHP code and suggest a fix for any issues (return types, types, bugs):\n\n```php\n" +
            snippet +
            "\n```"
        );
      },
    });

    // AI: generate a test for the current class.
    editor.addAction({
      id: "photon.aiTest",
      label: "Photon AI: Generate Test",
      contextMenuGroupId: "9_cutcopypaste",
      run: () =>
        onAiAsk(
          "Generate a thorough PHPUnit (or Pest if the project uses it) test for the class in the current file. Cover the public methods with realistic assertions."
        ),
    });

    // Local History — timeline of saved snapshots for this file.
    editor.addAction({
      id: "photon.localHistory",
      label: "Photon: Local History",
      contextMenuGroupId: "9_cutcopypaste",
      run: () => onLocalHistory(),
    });

    // Cmd/Ctrl+Shift+B → Go to Type Definition (the class of this expression).
    editor.addAction({
      id: "photon.gotoType",
      label: "Photon: Go to Type Definition",
      keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyCode.KeyB],
      run: () => {
        const pos = editor.getPosition();
        const model = editor.getModel();
        if (!pos || !model) return;
        const head = model.getValueInRange({
          startLineNumber: pos.lineNumber,
          startColumn: 1,
          endLineNumber: pos.lineNumber,
          endColumn: pos.column,
        });
        const m = head.match(/([$\\\w]+(?:(?:->|::|\?->)[\\\w]+(?:\([^()]*\))?)*)$/);
        onGotoType(m ? m[1] : "$this", model.getOffsetAt(pos));
      },
    });

    // F9 — toggle an Xdebug breakpoint on the cursor line.
    editor.addAction({
      id: "photon.toggleBreakpoint",
      label: "Photon: Toggle Breakpoint",
      keybindings: [monaco.KeyCode.F9],
      run: () => {
        const pos = editor.getPosition();
        if (pos) onToggleBreakpoint(pos.lineNumber);
      },
    });

    // Shift+F9 — conditional breakpoint (prompt for a PHP expression).
    editor.addAction({
      id: "photon.conditionalBreakpoint",
      label: "Photon: Add Conditional Breakpoint",
      keybindings: [monaco.KeyMod.Shift | monaco.KeyCode.F9],
      run: () => {
        const pos = editor.getPosition();
        if (pos) onConditionalBreakpoint(pos.lineNumber);
      },
    });

    // Click a ▶ test glyph in the gutter → run that test.
    editor.onMouseDown((e) => {
      if (e.target.type !== monaco.editor.MouseTargetType.GUTTER_GLYPH_MARGIN) return;
      const ln = e.target.position?.lineNumber;
      if (ln && testLinesRef.current.includes(ln)) onRunTest(ln);
    });

    // Cmd/Ctrl+click → floating "Show Usages" popup at the cursor.
    editor.onMouseDown((e) => {
      const me = e.event as {
        metaKey?: boolean;
        ctrlKey?: boolean;
        posx?: number;
        posy?: number;
      };
      if (!(me.metaKey || me.ctrlKey)) return;
      const pos = e.target.position;
      const model = editor.getModel();
      if (!pos || !model) return;
      const lineC = model.getLineContent(pos.lineNumber);
      // config('...') / route('...') / __('...') / env('...') → key definition.
      const ctx = keyContextAt(lineC, pos.column);
      if (ctx) {
        onResolveKey(ctx.kind, ctx.key);
        return;
      }
      const w = model.getWordAtPosition(pos);
      if (!w) return;
      const headB = lineC.slice(0, w.startColumn - 1);
      // app(Foo::class) / resolve() / make() → container binding (concrete).
      if (/^[A-Z]/.test(w.word) && /(?:app|resolve|make)\(\s*\\?[\w\\]*$/.test(headB)) {
        onResolveBinding(w.word);
        return;
      }
      // Member access? capture the receiver chain so the click can resolve to
      // the *right* class's member (symbol-resolved navigation).
      let chain: string | null = null;
      if (/(->|::)\s*$/.test(headB)) {
        const m = headB.match(/([$\\\w]+(?:(?:->|::)[\\\w]+(?:\([^()]*\))?)*)\s*(?:->|::)\s*$/);
        chain = m ? m[1] : "$this";
      }
      onCmdClick({
        word: w.word,
        line: pos.lineNumber,
        offset: model.getOffsetAt(pos),
        x: me.posx ?? 200,
        y: me.posy ?? 200,
        chain,
      });
    });

    // Register language providers once (shared across instances).
    if (!providersRegistered) {
      providersRegistered = true;
      registerProviders(monaco as unknown as typeof import("monaco-editor"));
    }
  };

  // Refresh completion data + diagnostics when the index changes / file opens.
  useEffect(() => {
    let cancelled = false;
    api.completionData().then((d) => { if (!cancelled) completionCache = d; }).catch(() => {});
    api.schemaTables().then((t) => { if (!cancelled) schemaCache = t; }).catch(() => {});
    api.bladeViews().then((v) => { if (!cancelled) bladeViews = v; }).catch(() => {});
    if (path && monacoRef.current && editorRef.current) {
      const monaco = monacoRef.current;
      const model = editorRef.current.getModel();
      api
        .lintFile(path)
        .then((diags) => {
          if (cancelled || !model) return;
          monaco.editor.setModelMarkers(
            model,
            "photon",
            diags.map((d) => ({
              startLineNumber: d.line,
              startColumn: d.col,
              endLineNumber: d.line,
              endColumn: d.end_col,
              message: d.message,
              severity:
                d.severity === "error"
                  ? monaco.MarkerSeverity.Error
                  : d.severity === "info"
                  ? monaco.MarkerSeverity.Info
                  : monaco.MarkerSeverity.Warning,
            }))
          );
        })
        .catch(() => {});
    }
    return () => { cancelled = true; };
  }, [path, lintKey]);

  // Breakpoint glyphs in the gutter.
  const bpDecos = useRef<string[]>([]);
  useEffect(() => {
    const ed = editorRef.current;
    const monaco = monacoRef.current;
    if (!ed || !monaco) return;
    bpDecos.current = ed.deltaDecorations(
      bpDecos.current,
      breakpointLines.map((ln) => ({
        range: new monaco.Range(ln, 1, ln, 1),
        options: { isWholeLine: false, glyphMarginClassName: "photon-bp" },
      }))
    );
  }, [breakpointLines, path]);

  // Test ▶ glyphs on test methods/classes (click in the gutter to run).
  const testLinesRef = useRef<number[]>([]);
  const testDecos = useRef<string[]>([]);
  useEffect(() => {
    testLinesRef.current = testLines;
    const ed = editorRef.current;
    const monaco = monacoRef.current;
    if (!ed || !monaco) return;
    testDecos.current = ed.deltaDecorations(
      testDecos.current,
      testLines.map((ln) => ({
        range: new monaco.Range(ln, 1, ln, 1),
        options: { glyphMarginClassName: "test-run", glyphMarginHoverMessage: { value: "Run test" } },
      }))
    );
  }, [testLines, path]);

  // Git diff bars in the gutter + overview-ruler markers.
  const gitDecos = useRef<string[]>([]);
  useEffect(() => {
    const ed = editorRef.current;
    const monaco = monacoRef.current;
    if (!ed || !monaco || !path) return;
    let cancelled = false;
    api
      .gitLineStatus(path)
      .then((st) => {
        if (cancelled || !ed.getModel()) return;
        const lane = monaco.editor.OverviewRulerLane.Left;
        const mk = (lines: number[], cls: string, color: string) =>
          lines.map((ln) => ({
            range: new monaco.Range(ln, 1, ln, 1),
            options: { linesDecorationsClassName: cls, overviewRuler: { color, position: lane } },
          }));
        gitDecos.current = ed.deltaDecorations(gitDecos.current, [
          ...mk(st.added, "gd-add", "#3fb95088"),
          ...mk(st.modified, "gd-mod", "#4c8bf588"),
          ...mk(st.deleted, "gd-del", "#ff5d5d88"),
        ]);
      })
      .catch(() => {});
    return () => { cancelled = true; };
  }, [path, lintKey]);

  // Debug: highlight the current stop line + inline evaluated values.
  const dbgDecos = useRef<string[]>([]);
  useEffect(() => {
    const ed = editorRef.current;
    const monaco = monacoRef.current;
    if (!ed || !monaco) return;
    if (debugLine && ed.getModel()) {
      dbgDecos.current = ed.deltaDecorations(dbgDecos.current, [
        {
          range: new monaco.Range(debugLine, 1, debugLine, 1),
          options: {
            isWholeLine: true,
            className: "dbg-line",
            after: debugInline ? { content: "   " + debugInline, inlineClassName: "dbg-inline" } : undefined,
          },
        },
      ]);
      ed.revealLineInCenter(debugLine);
    } else {
      dbgDecos.current = ed.deltaDecorations(dbgDecos.current, []);
    }
  }, [debugLine, debugInline]);

  useEffect(() => {
    if (reveal && reveal.line && editorRef.current) {
      const ed = editorRef.current;
      ed.revealLineInCenter(reveal.line);
      ed.setPosition({ lineNumber: reveal.line, column: 1 });
      ed.focus();
    }
  }, [reveal, path]);

  // Live-apply settings changes.
  useEffect(() => {
    editorRef.current?.updateOptions({
      fontSize: settings.editorFontSize,
      tabSize: settings.tabSize,
      wordWrap: settings.wordWrap ? "on" : "off",
      minimap: { enabled: settings.minimap },
      stickyScroll: { enabled: settings.stickyScroll },
      fontLigatures: settings.ligatures,
    });
  }, [settings]);

  // Native Vim mode — attach/detach on the live editor instance.
  useEffect(() => {
    const ed = editorRef.current;
    const mo = monacoRef.current;
    if (!ed || !mo) return;
    if (!settings.vimMode) {
      setVimState(null);
      return;
    }
    const ctrl = attachVim(ed, mo as unknown as typeof import("monaco-editor"), {
      onMode: (mode, cmd) => setVimState({ mode, cmd: cmd ?? "" }),
      onSave: () => onSaveRef.current(),
    });
    return () => ctrl.dispose();
  }, [settings.vimMode, mounted]);

  if (!path) {
    return (
      <div className="flex-1 flex items-center justify-center text-fg-faint flex-col gap-3">
        <div className="text-4xl font-light tracking-tight text-fg-muted">
          Photon
        </div>
        <div className="text-sm">
          Open a file from the sidebar, or press{" "}
          <span className="kbd">Shift Shift</span> to search everywhere.
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 relative min-h-0">
    {debugActive && (
      <div
        className="absolute top-2 right-4 z-10 flex items-center gap-2 px-2.5 py-1.5 rounded-lg"
        style={{ background: "rgba(43,45,48,0.92)", border: "1px solid #3a3d42", backdropFilter: "blur(6px)" }}
      >
        <button title="Resume (F5)" onClick={() => api.debugCommand("run")} className="text-[#61c554] hover:scale-110">▶</button>
        <button title="Step over (F10)" onClick={() => api.debugCommand("step_over")} className="text-fg-muted hover:text-fg">⤼</button>
        <button title="Step into (F11)" onClick={() => api.debugCommand("step_into")} className="text-fg-muted hover:text-fg">↳</button>
        <button title="Step out (⇧F11)" onClick={() => api.debugCommand("step_out")} className="text-fg-muted hover:text-fg">↰</button>
        <button title="Stop" onClick={() => api.debugCommand("stop")} className="text-[#ff5d5d] hover:scale-110">■</button>
      </div>
    )}
    <Editor
      className="absolute inset-0"
      theme="photon-dark"
      path={path}
      language={language}
      value={value}
      onMount={handleMount}
      onChange={(v) => onChange(v ?? "")}
      options={{
        fontSize: settings.editorFontSize,
        tabSize: settings.tabSize,
        wordWrap: settings.wordWrap ? "on" : "off",
        fontFamily: "JetBrains Mono, SFMono-Regular, Menlo, monospace",
        fontLigatures: settings.ligatures,
        minimap: { enabled: settings.minimap, renderCharacters: false },
        stickyScroll: { enabled: settings.stickyScroll },
        smoothScrolling: true,
        cursorBlinking: "smooth",
        cursorSmoothCaretAnimation: "on",
        renderWhitespace: "selection",
        scrollBeyondLastLine: false,
        padding: { top: 10 },
        glyphMargin: true,
        bracketPairColorization: { enabled: true },
        guides: { bracketPairs: true, indentation: true },
        multiCursorModifier: "ctrlCmd",
      }}
    />
    {vimState && (
      <div className="absolute bottom-0 left-0 right-0 h-6 flex items-center gap-3 px-3 text-2xs font-mono bg-bg-panel/95 border-t border-line z-10">
        {vimState.mode === "command" ? (
          <span className="text-fg">{vimState.cmd || ":"}</span>
        ) : (
          <span
            className="font-semibold tracking-wider"
            style={{
              color:
                vimState.mode === "insert"
                  ? "#3fb950"
                  : vimState.mode === "visual"
                  ? "#d29922"
                  : "#3574f0",
            }}
          >
            -- {vimState.mode.toUpperCase()} --
          </span>
        )}
        <span className="ml-auto text-fg-faint">VIM</span>
      </div>
    )}
    </div>
  );
}
