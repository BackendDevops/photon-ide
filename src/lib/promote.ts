// PHP 8 constructor property promotion — a conservative, review-friendly
// transform. Given the full file text and the cursor line (1-based), if the
// cursor is on a `__construct` whose params are assigned to `$this->x = $x`,
// it returns the rewritten file text with promoted params (and the now-redundant
// property declarations + assignments removed). Returns null when not applicable.

interface Edit {
  start: number;
  end: number;
  text: string;
}

const VIS = /^(public|protected|private)\b/;

// Split a parameter list on top-level commas (ignoring (), [], {} and strings).
function splitParams(s: string): string[] {
  const out: string[] = [];
  let depth = 0;
  let buf = "";
  let quote: string | null = null;
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
    if (quote) {
      buf += c;
      if (c === quote && s[i - 1] !== "\\") quote = null;
      continue;
    }
    if (c === '"' || c === "'") {
      quote = c;
      buf += c;
      continue;
    }
    if (c === "(" || c === "[" || c === "{") depth++;
    if (c === ")" || c === "]" || c === "}") depth--;
    if (c === "," && depth === 0) {
      out.push(buf);
      buf = "";
    } else buf += c;
  }
  if (buf.trim()) out.push(buf);
  return out;
}

function matchClose(code: string, open: number, oc: string, cc: string): number {
  let depth = 0;
  for (let i = open; i < code.length; i++) {
    if (code[i] === oc) depth++;
    else if (code[i] === cc) {
      depth--;
      if (depth === 0) return i;
    }
  }
  return -1;
}

// Whole-line bounds [start,end) including the trailing newline, for a char index.
function lineBounds(code: string, idx: number): { start: number; end: number } {
  let start = code.lastIndexOf("\n", idx - 1) + 1;
  let end = code.indexOf("\n", idx);
  end = end === -1 ? code.length : end + 1;
  return { start, end };
}

export function promoteConstructor(code: string, line: number): string | null {
  const lines = code.split("\n");
  if (line < 1 || line > lines.length) return null;

  // Find the __construct signature whose '(' is on or nearest the cursor line.
  const ctorRe = /function\s+__construct\s*\(/g;
  let m: RegExpExecArray | null;
  let sigParenOpen = -1;
  while ((m = ctorRe.exec(code))) {
    const ln = code.slice(0, m.index).split("\n").length;
    if (Math.abs(ln - line) <= 6) {
      sigParenOpen = m.index + m[0].length - 1;
      break;
    }
    if (sigParenOpen === -1) sigParenOpen = m.index + m[0].length - 1; // fallback: first
  }
  if (sigParenOpen === -1) return null;

  const sigParenClose = matchClose(code, sigParenOpen, "(", ")");
  if (sigParenClose === -1) return null;
  const braceOpen = code.indexOf("{", sigParenClose);
  if (braceOpen === -1) return null;
  const braceClose = matchClose(code, braceOpen, "{", "}");
  if (braceClose === -1) return null;

  const paramsStr = code.slice(sigParenOpen + 1, sigParenClose);
  const body = code.slice(braceOpen + 1, braceClose);
  const params = splitParams(paramsStr);
  if (params.length === 0) return null;

  const ctorIndent = (lines[line - 1].match(/^\s*/)?.[0] ?? "    ");
  const pIndent = ctorIndent + "    ";

  const edits: Edit[] = [];
  const newParams: string[] = [];
  let promotedCount = 0;

  for (const raw of params) {
    const param = raw.trim();
    if (!param) continue;
    // Already promoted?
    if (VIS.test(param)) {
      newParams.push(param);
      continue;
    }
    const beforeDefault = param.split("=")[0];
    const nameMatch = beforeDefault.match(/\$(\w+)\s*$/);
    if (!nameMatch) {
      newParams.push(param);
      continue;
    }
    const name = nameMatch[1];

    // Assignment $this->name = $name;
    const assignRe = new RegExp(`\\$this->${name}\\s*=\\s*\\$${name}\\s*;`);
    const am = assignRe.exec(body);
    if (!am) {
      newParams.push(param);
      continue;
    }

    const eq = param.indexOf("=");
    const def = eq >= 0 ? param.slice(eq + 1).trim() : null;

    // Property declaration (optional): adopt its visibility/modifiers/type and
    // delete the declaration. The promoted param then carries only the name.
    const declRe = new RegExp(
      `(^|\\n)([ \\t]*)((?:public|protected|private)\\s+(?:readonly\\s+)?(?:static\\s+)?[^\\n;=]*?)\\$${name}\\s*(?:=[^;]*)?;`,
    );
    const dm = declRe.exec(code);
    let promoted: string;
    if (dm) {
      const declVisType = dm[3].trim(); // "private UserRepository" / "protected readonly Foo"
      promoted = `${declVisType} $${name}` + (def ? ` = ${def}` : "");
      const declIdx = dm.index + dm[1].length;
      const lb = lineBounds(code, declIdx);
      edits.push({ start: lb.start, end: lb.end, text: "" });
    } else {
      // No declaration: keep the param's own type, just add visibility.
      promoted = `private ${param}`;
    }

    // Remove the assignment line.
    const lbAssign = lineBounds(body, am.index);
    edits.push({
      start: braceOpen + 1 + lbAssign.start,
      end: braceOpen + 1 + lbAssign.end,
      text: "",
    });

    newParams.push(promoted.replace(/\s+/g, " ").trim());
    promotedCount++;
  }

  if (promotedCount === 0) return null;

  // Rebuild the param list (multi-line for readability).
  const joined =
    newParams.length > 1
      ? "\n" + newParams.map((p) => pIndent + p).join(",\n") + "\n" + ctorIndent
      : newParams[0];
  edits.push({ start: sigParenOpen + 1, end: sigParenClose, text: joined });

  // Apply edits right-to-left so indices stay valid.
  edits.sort((a, b) => b.start - a.start);
  let out = code;
  for (const e of edits) out = out.slice(0, e.start) + e.text + out.slice(e.end);

  // Collapse blank lines left where the constructor body emptied out.
  out = out.replace(/\{\s*\n\s*\n+/g, "{\n").replace(/\n[ \t]*\n[ \t]*\}/g, "\n    }");
  return out === code ? null : out;
}
