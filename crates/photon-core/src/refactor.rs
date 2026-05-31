//! Refactoring engine (MVP-v1 slice of `docs/02-module-design.md` §refactoring).
//!
//! Implements **plan-then-apply** Safe Rename: compute a full `ChangeSet`
//! against the index (definitions + references), let the UI preview it, then
//! apply atomically. References whose target is ambiguous (the same short name
//! defined in more than one place) are flagged `certain = false` so the user
//! can confirm — correctness over silent breakage.

use crate::db::Index;
use crate::types::{ChangeSet, TextEdit};

/// Build a rename plan for `old` → `new` across the whole project.
///
/// `read` returns the current text of a workspace-relative file (for previews
/// and to apply edits); callers pass the workspace reader.
pub fn plan_rename<R: Fn(&str) -> Option<String>>(
    index: &Index,
    old: &str,
    new: &str,
    read: R,
) -> anyhow::Result<ChangeSet> {
    let defs = index.find_symbol(old)?;
    let refs = index.references_to(old)?;
    // Ambiguous if the short name is defined in more than one place.
    let certain_globally = defs.len() <= 1;

    let mut edits: Vec<TextEdit> = Vec::new();
    let mut file_cache: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();

    let mut line_preview = |file: &str, line: u32, replace_with: &str, col_name: &str| -> String {
        let lines = file_cache
            .entry(file.to_string())
            .or_insert_with(|| read(file).unwrap_or_default().split('\n').map(|s| s.to_string()).collect());
        let idx = (line as usize).saturating_sub(1);
        let original = lines.get(idx).cloned().unwrap_or_default();
        // Show the line with the rename applied (first occurrence of the name).
        original.replacen(col_name, replace_with, 1).trim_end().to_string()
    };

    // Definition sites (the declared name itself).
    for d in &defs {
        let start = d.name_offset;
        let end = d.name_offset + old.len() as u32;
        edits.push(TextEdit {
            file: d.file.clone(),
            start,
            end,
            line: d.line,
            new_text: new.to_string(),
            preview: line_preview(&d.file, d.line, new, old),
            certain: true,
        });
    }

    // Use sites.
    for r in &refs {
        // Skip a ref that exactly coincides with a definition edit (defensive).
        if edits
            .iter()
            .any(|e| e.file == r.file && e.start == r.start)
        {
            continue;
        }
        edits.push(TextEdit {
            file: r.file.clone(),
            start: r.start,
            end: r.end,
            line: r.line,
            new_text: new.to_string(),
            preview: line_preview(&r.file, r.line, new, old),
            certain: certain_globally,
        });
    }

    let mut files: Vec<&String> = edits.iter().map(|e| &e.file).collect();
    files.sort();
    files.dedup();

    Ok(ChangeSet {
        title: format!("Rename '{}' to '{}'", old, new),
        files_affected: files.len() as u32,
        edits,
    })
}

/// Extract the selected expression into a new variable declared on its own
/// line just above, and replace the selection with the variable.
pub fn plan_extract_variable(
    content: &str,
    file: &str,
    sel_start: u32,
    sel_end: u32,
    new_name: &str,
    line: u32,
) -> ChangeSet {
    let s = sel_start as usize;
    let e = (sel_end as usize).min(content.len());
    let selected = if s <= e { &content[s..e] } else { "" };

    // Line start + indentation of the statement we extract from.
    let line_start = content[..s].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let indent: String = content[line_start..]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect();

    let var = format!("${}", new_name.trim_start_matches('$'));
    let decl = format!("{indent}{var} = {selected};\n");

    let edits = vec![
        TextEdit {
            file: file.to_string(),
            start: line_start as u32,
            end: line_start as u32,
            line,
            new_text: decl.clone(),
            preview: decl.trim_end().to_string(),
            certain: true,
        },
        TextEdit {
            file: file.to_string(),
            start: sel_start,
            end: sel_end,
            line,
            new_text: var.clone(),
            preview: format!("… {} …", var),
            certain: true,
        },
    ];
    ChangeSet {
        title: format!("Extract variable {}", var),
        files_affected: 1,
        edits,
    }
}

/// Inline a local `$var = EXPR;` — remove the declaration and replace each use
/// of `$var` with `EXPR`. Best-effort (text-scoped); uses are flagged uncertain.
pub fn plan_inline_variable(content: &str, file: &str, var: &str) -> ChangeSet {
    let bare = var.trim_start_matches('$');
    let decl_token = format!("${} =", bare);
    let decl_token_alt = format!("${}=", bare);
    let mut edits = Vec::new();

    // Locate the assignment.
    let assign = content.find(&decl_token).or_else(|| content.find(&decl_token_alt));
    let mut expr = String::new();
    if let Some(at) = assign {
        let after_eq = content[at..].find('=').map(|i| at + i + 1).unwrap_or(at);
        let semi = content[after_eq..].find(';').map(|i| after_eq + i).unwrap_or(content.len());
        expr = content[after_eq..semi].trim().to_string();
        // delete the whole declaration line
        let line_start = content[..at].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let line_end = content[semi..].find('\n').map(|i| semi + i + 1).unwrap_or(content.len());
        let line_no = (content[..line_start].matches('\n').count() + 1) as u32;
        edits.push(TextEdit {
            file: file.to_string(),
            start: line_start as u32,
            end: line_end as u32,
            line: line_no,
            new_text: String::new(),
            preview: format!("(remove) ${} = {};", bare, expr),
            certain: true,
        });
    }

    // Replace remaining uses of $var (word-boundary) with the expression.
    if !expr.is_empty() {
        let needle = format!("${}", bare);
        let bytes = content.as_bytes();
        let mut idx = 0usize;
        while let Some(p) = content[idx..].find(&needle) {
            let at = idx + p;
            idx = at + needle.len();
            // boundary: next char must not be ident char (avoid $userName matching $user)
            let next_ok = bytes
                .get(at + needle.len())
                .map(|&b| !(b as char).is_alphanumeric() && b != b'_')
                .unwrap_or(true);
            if !next_ok {
                continue;
            }
            // skip the declaration occurrence (it is being removed)
            if let Some(a) = assign {
                if at == a {
                    continue;
                }
            }
            let line_no = (content[..at].matches('\n').count() + 1) as u32;
            edits.push(TextEdit {
                file: file.to_string(),
                start: at as u32,
                end: (at + needle.len()) as u32,
                line: line_no,
                new_text: expr.clone(),
                preview: expr.clone(),
                certain: false,
            });
        }
    }

    ChangeSet {
        title: format!("Inline variable ${}", bare),
        files_affected: 1,
        edits,
    }
}

/// Extract the selected statements into a new private method, inserted after
/// the enclosing method (at `insert_at`), and replace the selection with a call.
/// `$vars` referenced in the selection (except `$this`) become parameters.
/// Best-effort (statement blocks); the call site is flagged uncertain.
pub fn plan_extract_method(
    content: &str,
    file: &str,
    sel_start: u32,
    sel_end: u32,
    method_name: &str,
    insert_at: u32,
    indent: &str,
    line: u32,
) -> ChangeSet {
    let s = sel_start as usize;
    let e = (sel_end as usize).min(content.len());
    let selected = if s <= e { &content[s..e] } else { "" };

    // Collect distinct `$vars` used in the selection (params), excluding $this.
    let mut vars: Vec<String> = Vec::new();
    let bytes = selected.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'$' {
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_') {
                j += 1;
            }
            if j > start {
                let v = format!("${}", &selected[start..j]);
                if v != "$this" && !vars.contains(&v) {
                    vars.push(v);
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    let params = vars.join(", ");
    let body_indent = format!("{indent}    ");
    let body: String = selected
        .lines()
        .map(|l| {
            if l.trim().is_empty() {
                String::new()
            } else {
                format!("{body_indent}{}", l.trim_start())
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let method = format!(
        "\n\n{indent}private function {method_name}({params})\n{indent}{{\n{body}\n{indent}}}",
    );
    let call = format!("$this->{method_name}({params});");

    ChangeSet {
        title: format!("Extract method {}()", method_name),
        files_affected: 1,
        edits: vec![
            TextEdit {
                file: file.to_string(),
                start: insert_at,
                end: insert_at,
                line,
                new_text: method,
                preview: format!("+ private function {}({})", method_name, params),
                certain: true,
            },
            TextEdit {
                file: file.to_string(),
                start: sel_start,
                end: sel_end,
                line,
                new_text: call.clone(),
                preview: call,
                certain: false,
            },
        ],
    }
}

/// Push replace edits for `needle` in `content`, requiring an identifier/`\`
/// boundary after it (so `Foo` won't match `Foobar` or `Foo\Bar`).
fn push_replace_token(
    content: &str,
    file: &str,
    needle: &str,
    repl: &str,
    certain: bool,
    edits: &mut Vec<TextEdit>,
) {
    if needle.is_empty() {
        return;
    }
    let bytes = content.as_bytes();
    let mut i = 0usize;
    while let Some(p) = content[i..].find(needle) {
        let at = i + p;
        i = at + needle.len();
        let after = bytes.get(at + needle.len()).copied();
        let ok = after
            .map(|b| !(b.is_ascii_alphanumeric() || b == b'_' || b == b'\\'))
            .unwrap_or(true);
        if !ok {
            continue;
        }
        let line = (content[..at].matches('\n').count() + 1) as u32;
        edits.push(TextEdit {
            file: file.to_string(),
            start: at as u32,
            end: (at + needle.len()) as u32,
            line,
            new_text: repl.to_string(),
            preview: repl.to_string(),
            certain,
        });
    }
}

/// Move a class to a new namespace: rewrite its `namespace …;` declaration and
/// update every `use Old\Fqn` import and `\Old\Fqn` fully-qualified reference
/// across the project to the new namespace.
pub fn plan_move_class<R: Fn(&str) -> Option<String>>(
    index: &Index,
    class: &str,
    new_ns: &str,
    files: &[String],
    read: R,
) -> anyhow::Result<ChangeSet> {
    use crate::types::SymbolKind;
    let def = index
        .find_symbol(class)?
        .into_iter()
        .find(|d| {
            d.fqn.is_some()
                && matches!(
                    d.kind,
                    SymbolKind::Class | SymbolKind::Interface | SymbolKind::Trait | SymbolKind::Enum
                )
        })
        .ok_or_else(|| anyhow::anyhow!("class '{class}' not found"))?;
    let old_fqn = def.fqn.clone().unwrap();
    let old_ns = old_fqn.rsplitn(2, '\\').nth(1).unwrap_or("").to_string();
    let new_ns = new_ns.trim().trim_end_matches('\\').to_string();
    if new_ns == old_ns {
        return Ok(ChangeSet { title: "Move class (no change)".into(), files_affected: 0, edits: vec![] });
    }
    let new_fqn = if new_ns.is_empty() {
        class.to_string()
    } else {
        format!("{new_ns}\\{class}")
    };

    let mut edits = Vec::new();
    // 1) the `namespace` declaration in the class's own file.
    if let Some(content) = read(&def.file) {
        let needle = format!("namespace {old_ns}");
        if let Some(p) = content.find(&needle) {
            let line = (content[..p].matches('\n').count() + 1) as u32;
            let repl = if new_ns.is_empty() {
                "namespace ".to_string()
            } else {
                format!("namespace {new_ns}")
            };
            edits.push(TextEdit {
                file: def.file.clone(),
                start: p as u32,
                end: (p + needle.len()) as u32,
                line,
                new_text: repl.clone(),
                preview: repl,
                certain: true,
            });
        }
    }
    // 2) imports + fully-qualified refs across the project.
    for f in files {
        if let Some(content) = read(f) {
            push_replace_token(&content, f, &format!("use {old_fqn}"), &format!("use {new_fqn}"), true, &mut edits);
            push_replace_token(&content, f, &format!("\\{old_fqn}"), &format!("\\{new_fqn}"), false, &mut edits);
        }
    }

    let mut affected: Vec<&String> = edits.iter().map(|e| &e.file).collect();
    affected.sort();
    affected.dedup();
    Ok(ChangeSet {
        title: format!("Move {class} to {}", if new_ns.is_empty() { "global namespace" } else { &new_ns }),
        files_affected: affected.len() as u32,
        edits,
    })
}

/// Replace a function/method's parameter list (the text between its `(` and the
/// matching `)`) with `new_params`. `decl_start` is the byte offset of the
/// declaration. Call sites are left to the developer (reviewed via the diff).
pub fn plan_change_signature(
    content: &str,
    file: &str,
    decl_start: u32,
    new_params: &str,
    line: u32,
) -> ChangeSet {
    let s = decl_start as usize;
    let open = match content[s.min(content.len())..].find('(') {
        Some(o) => s + o,
        None => return ChangeSet { title: "Change signature".into(), files_affected: 0, edits: vec![] },
    };
    let mut depth = 0i32;
    let mut close = content.len();
    for (i, ch) in content[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    close = open + i;
                    break;
                }
            }
            _ => {}
        }
    }
    let edits = vec![TextEdit {
        file: file.to_string(),
        start: (open + 1) as u32,
        end: close as u32,
        line,
        new_text: new_params.to_string(),
        preview: format!("({new_params})"),
        certain: true,
    }];
    ChangeSet { title: "Change signature".into(), files_affected: 1, edits }
}

/// Apply a change set to file contents in memory, returning the new content
/// per file. Edits are applied per file from the highest offset down so earlier
/// offsets remain valid. Only `accepted` edits (by index into `cs.edits`) are
/// applied; pass `None` to apply all.
pub fn apply_changeset(
    cs: &ChangeSet,
    accepted: Option<&[usize]>,
    read: impl Fn(&str) -> Option<String>,
) -> anyhow::Result<Vec<(String, String)>> {
    use std::collections::HashMap;

    let accept = |i: usize| accepted.map(|a| a.contains(&i)).unwrap_or(true);

    // Group accepted edits by file.
    let mut by_file: HashMap<String, Vec<&TextEdit>> = HashMap::new();
    for (i, e) in cs.edits.iter().enumerate() {
        if accept(i) {
            by_file.entry(e.file.clone()).or_default().push(e);
        }
    }

    let mut results = Vec::new();
    for (file, mut file_edits) in by_file {
        let mut content = read(&file).ok_or_else(|| anyhow::anyhow!("cannot read {file}"))?;
        // Apply from the end backwards.
        file_edits.sort_by(|a, b| b.start.cmp(&a.start));
        for e in file_edits {
            let s = e.start as usize;
            let t = e.end as usize;
            if s <= t && t <= content.len() && content.is_char_boundary(s) && content.is_char_boundary(t) {
                content.replace_range(s..t, &e.new_text);
            }
        }
        results.push((file, content));
    }
    Ok(results)
}
