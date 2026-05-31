//! Inspection engine v1 (docs/18 §Inspections).
//!
//! Pure, text-level inspections — high-signal and low-false-positive by design,
//! so they can run on every save without annoying the developer. Each returns
//! `Diagnostic`s the editor renders as squiggles (and pairs with a quick-fix
//! where it makes sense). Type-driven inspections (undefined member, type
//! mismatch) build on the type engine in a later step.

use crate::types::Diagnostic;

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Count word-boundary occurrences of `word` in `src`.
fn count_word(src: &str, word: &str) -> usize {
    if word.is_empty() {
        return 0;
    }
    let bytes = src.as_bytes();
    let mut n = 0;
    let mut i = 0;
    while let Some(p) = src[i..].find(word) {
        let at = i + p;
        i = at + word.len();
        let before_ok = at == 0 || !is_ident_byte(bytes[at - 1]);
        let after_ok = bytes.get(at + word.len()).map(|b| !is_ident_byte(*b)).unwrap_or(true);
        if before_ok && after_ok {
            n += 1;
        }
    }
    n
}

fn parse_use(line: &str) -> Option<String> {
    let l = line.trim_start();
    let rest = l.strip_prefix("use ")?.trim();
    let rest = rest.trim_end_matches(';').trim();
    if rest.contains('{') || rest.starts_with("function ") || rest.starts_with("const ") {
        return None; // group/function/const use — skip for v1
    }
    Some(rest.to_string())
}

fn short_name(fqn: &str) -> &str {
    if let Some(p) = fqn.find(" as ") {
        return fqn[p + 4..].trim();
    }
    fqn.rsplit('\\').next().unwrap_or(fqn)
}

/// `use X\Y\Z;` whose short name is never used elsewhere in the file.
pub fn unused_imports(source: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for (i, line) in source.lines().enumerate() {
        let fqn = match parse_use(line) {
            Some(f) => f,
            None => continue,
        };
        let short = short_name(&fqn);
        if short.is_empty() {
            continue;
        }
        // The `use` line itself contributes one occurrence; >1 means it's used
        // somewhere (code or PHPDoc). The text scan avoids PHPDoc false-positives.
        if count_word(source, short) <= 1 {
            let col = line.find("use").map(|c| c as u32 + 1).unwrap_or(1);
            out.push(Diagnostic {
                line: i as u32 + 1,
                col,
                end_col: line.trim_end().chars().count() as u32 + 1,
                message: format!("Unused import '{}'", short),
                severity: "warning".into(),
            });
        }
    }
    out
}

/// The same fully-qualified import appearing more than once.
pub fn duplicate_imports(source: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (i, line) in source.lines().enumerate() {
        if let Some(fqn) = parse_use(line) {
            if !seen.insert(fqn.clone()) {
                let col = line.find("use").map(|c| c as u32 + 1).unwrap_or(1);
                out.push(Diagnostic {
                    line: i as u32 + 1,
                    col,
                    end_col: line.trim_end().chars().count() as u32 + 1,
                    message: format!("Duplicate import '{}'", short_name(&fqn)),
                    severity: "warning".into(),
                });
            }
        }
    }
    out
}

const DEBUG_CALLS: &[&str] = &["dd", "dump", "var_dump", "ray", "dde", "print_r", "var_export"];

/// Leftover debug calls (`dd()`, `dump()`, `var_dump()`, `ray()`, …) that are
/// not method calls (`$x->dump()` is allowed).
pub fn debug_statements(source: &str) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    for call in DEBUG_CALLS {
        let needle = format!("{}(", call);
        let mut i = 0;
        while let Some(p) = source[i..].find(&needle) {
            let at = i + p;
            i = at + needle.len();
            // Boundary before the call: not part of an identifier and not a
            // member/static access.
            let ok = if at == 0 {
                true
            } else {
                let b = bytes[at - 1];
                !is_ident_byte(b) && b != b'>' && b != b':' && b != b'\\'
            };
            if !ok {
                continue;
            }
            let before = &source[..at];
            let line = before.matches('\n').count() as u32 + 1;
            let col = (at - before.rfind('\n').map(|x| x + 1).unwrap_or(0)) as u32 + 1;
            out.push(Diagnostic {
                line,
                col,
                end_col: col + call.chars().count() as u32,
                message: format!("Leftover debug statement '{}()'", call),
                severity: "warning".into(),
            });
        }
    }
    out
}

/// Run all file-local inspections.
pub fn inspect_file(source: &str) -> Vec<Diagnostic> {
    let mut out = unused_imports(source);
    out.extend(duplicate_imports(source));
    out.extend(debug_statements(source));
    // Structural (tree-sitter): unreachable code + missing return type.
    out.extend(crate::php::analysis_diagnostics(source));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_unused_import() {
        let src = "<?php\nuse App\\Models\\User;\nuse App\\Models\\Post;\n\nfunction f(User $u) {}\n";
        let d = unused_imports(src);
        assert!(d.iter().any(|x| x.message.contains("Post")));
        assert!(!d.iter().any(|x| x.message.contains("User")));
    }

    #[test]
    fn flags_duplicate_import() {
        let src = "<?php\nuse App\\Models\\User;\nuse App\\Models\\User;\n";
        let d = duplicate_imports(src);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn flags_unreachable_code_after_return() {
        let src = "<?php\nfunction f(): int {\n    return 1;\n    $x = 2;\n}\n";
        let d = crate::php::analysis_diagnostics(src);
        assert!(d.iter().any(|x| x.message == "Unreachable code"), "{d:?}");
    }

    #[test]
    fn no_unreachable_for_conditional_return() {
        let src = "<?php\nfunction f(int $n): int {\n    if ($n > 0) {\n        return 1;\n    }\n    return 2;\n}\n";
        let d = crate::php::analysis_diagnostics(src);
        assert!(!d.iter().any(|x| x.message == "Unreachable code"), "{d:?}");
    }

    #[test]
    fn flags_missing_return_type_when_value_returned() {
        let src = "<?php\nclass A {\n    public function name() {\n        return 'x';\n    }\n    public function typed(): string {\n        return 'y';\n    }\n    public function nothing() {\n        $a = 1;\n    }\n}\n";
        let d = crate::php::analysis_diagnostics(src);
        assert!(d.iter().any(|x| x.message.contains("Missing return type on 'name()'")), "{d:?}");
        assert!(!d.iter().any(|x| x.message.contains("typed()")), "{d:?}");
        assert!(!d.iter().any(|x| x.message.contains("nothing()")), "{d:?}");
    }

    #[test]
    fn flags_readonly_reassignment_outside_ctor() {
        let src = "<?php\nclass Money {\n    public function __construct(public readonly int $amount) {}\n    public function set(int $n): void {\n        $this->amount = $n;\n    }\n}\n";
        let d = crate::php::analysis_diagnostics(src);
        assert!(d.iter().any(|x| x.message.contains("readonly property '$this->amount'")), "{d:?}");
    }

    #[test]
    fn allows_readonly_assignment_in_ctor() {
        let src = "<?php\nclass Money {\n    public readonly int $amount;\n    public function __construct(int $n) {\n        $this->amount = $n;\n    }\n}\n";
        let d = crate::php::analysis_diagnostics(src);
        assert!(!d.iter().any(|x| x.message.contains("readonly")), "{d:?}");
    }

    #[test]
    fn flags_non_exhaustive_enum_match() {
        let src = "<?php\nenum Status { case Active; case Inactive; case Pending; }\nfunction label(Status $s): string {\n    return match($s) {\n        Status::Active => 'a',\n        Status::Inactive => 'i',\n    };\n}\n";
        let d = crate::php::analysis_diagnostics(src);
        assert!(d.iter().any(|x| x.message.contains("not exhaustive") && x.message.contains("Pending")), "{d:?}");
    }

    #[test]
    fn exhaustive_or_default_match_is_ok() {
        let src = "<?php\nenum Status { case Active; case Inactive; }\nfunction label(Status $s): string {\n    return match($s) {\n        Status::Active => 'a',\n        default => 'x',\n    };\n}\n";
        let d = crate::php::analysis_diagnostics(src);
        assert!(!d.iter().any(|x| x.message.contains("not exhaustive")), "{d:?}");
    }

    #[test]
    fn flags_unused_private_members() {
        let src = "<?php\nclass A {\n    private int $used = 0;\n    private int $dead = 0;\n    private function helper(): void {}\n    public function run(): void {\n        $this->used = 1;\n    }\n}\n";
        let d = crate::php::analysis_diagnostics(src);
        assert!(d.iter().any(|x| x.message.contains("Unused private property 'dead'")), "{d:?}");
        assert!(d.iter().any(|x| x.message.contains("Unused private method 'helper'")), "{d:?}");
        assert!(!d.iter().any(|x| x.message.contains("'used'")), "{d:?}");
    }

    #[test]
    fn flags_debug_statements_but_not_methods() {
        let src = "<?php\ndd($x);\n$collection->dump();\nvar_dump($y);\n";
        let d = debug_statements(src);
        assert!(d.iter().any(|x| x.message.contains("dd()")));
        assert!(d.iter().any(|x| x.message.contains("var_dump()")));
        // `$collection->dump()` is a method call → not flagged
        assert_eq!(d.iter().filter(|x| x.message.contains("dump()") && !x.message.contains("var_dump")).count(), 0);
    }
}
