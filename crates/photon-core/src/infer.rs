//! Pragmatic type inference for member completion (v2 W1 — docs/17, docs/05).
//!
//! Not a full type lattice yet: resolves the *receiver* of a `->` / `::` access
//! to a class name using the body-range index plus light source scanning
//! (`$this`/`self`, typed params, typed properties, `$x = new Foo`). That's
//! enough to drive accurate member completion — the headline v2 feature — and
//! is a clean base for the full engine.

use crate::types::{Symbol, SymbolKind};

/// The innermost class/interface/trait/enum whose declaration covers `offset`.
pub fn enclosing_class(symbols: &[Symbol], offset: u32) -> Option<String> {
    symbols
        .iter()
        .filter(|s| {
            matches!(
                s.kind,
                SymbolKind::Class | SymbolKind::Interface | SymbolKind::Trait | SymbolKind::Enum
            )
        })
        .filter(|s| s.range_end > s.range_start && s.range_start <= offset && offset <= s.range_end)
        .min_by_key(|s| s.range_end - s.range_start)
        .map(|s| s.name.clone())
}

/// Infer the class type of a `$var` from the source:
/// - `$var = new Foo(…)`
/// - `$var = app(Foo::class)` / `resolve(Foo::class)` / `->make(Foo::class)`
/// - typed parameter `Foo $var`
/// - typed property `private Foo $var`
pub fn infer_var_type(source: &str, var_with_dollar: &str) -> Option<String> {
    let var = var_with_dollar;
    let mut idx = 0usize;
    let mut typed: Option<String> = None;
    while let Some(p) = source[idx..].find(var) {
        let at = idx + p;
        idx = at + var.len();
        // boundary: next char must not continue the identifier ($user vs $username)
        let next_ok = source[idx..]
            .chars()
            .next()
            .map(|c| !c.is_alphanumeric() && c != '_')
            .unwrap_or(true);
        if !next_ok {
            continue;
        }
        let after = source[idx..].trim_start();
        // `$var = <expr>`
        if let Some(rest) = after.strip_prefix('=') {
            let rest = rest.trim_start();
            // `$var = new Foo(`
            if let Some(r2) = rest.strip_prefix("new ") {
                let ty = read_type(r2.trim_start());
                if !ty.is_empty() {
                    return Some(short(&ty)); // assignment wins
                }
            }
            // `$var = app(Foo::class)` / `resolve(Foo::class)` / `->make(Foo::class)`
            if let Some(class) = extract_container_call(rest) {
                return Some(class);
            }
            continue;
        }
        // typed param / property: a type token immediately precedes `$var`
        if typed.is_none() {
            let line_start = source[..at].rfind('\n').map(|i| i + 1).unwrap_or(0);
            let before = source[line_start..at].trim_end();
            if let Some(tok) = before.rsplit([' ', '(', ',']).find(|s| !s.is_empty()) {
                let tok = tok.trim_start_matches('?');
                if is_type_token(tok) {
                    typed = Some(short(tok));
                }
            }
        }
    }
    typed
}

/// Extract the class argument from a service-container call on the same
/// assignment RHS. Handles:
/// - `app(Foo::class)`, `resolve(Foo::class)`
/// - `$this->app->make(Foo::class)`, `App::make(Foo::class)`
/// - `app()->make(Foo::class)`, `$app->makeWith(Foo::class)`
fn extract_container_call(s: &str) -> Option<String> {
    let class_pos = s.find("::class")?;
    let before = &s[..class_pos];
    // Walk backwards to extract the class name token.
    let class_name: String = before
        .chars()
        .rev()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '\\')
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    if class_name.is_empty() {
        return None;
    }
    let prefix = &before[..before.len() - class_name.len()];
    let is_container = prefix.contains("app(")
        || prefix.contains("resolve(")
        || prefix.contains("->make(")
        || prefix.contains("::make(")
        || prefix.contains("->makeWith(");
    if is_container { Some(short(&class_name)) } else { None }
}

fn read_type(s: &str) -> String {
    s.chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '\\')
        .collect()
}

fn short(fqn: &str) -> String {
    fqn.trim_start_matches('\\')
        .rsplit('\\')
        .next()
        .unwrap_or(fqn)
        .to_string()
}

/// A token that looks like a class type (Capitalized, not a keyword/modifier).
fn is_type_token(tok: &str) -> bool {
    if tok.is_empty() {
        return false;
    }
    let bare = short(tok);
    let lower = bare.to_lowercase();
    let reserved = [
        "public", "private", "protected", "static", "function", "return", "var", "const",
        "new", "int", "float", "string", "bool", "array", "void", "mixed", "object",
        "callable", "iterable", "null", "false", "true", "self", "static", "parent",
        "readonly", "final", "abstract", "echo", "if", "else", "foreach", "for", "while",
    ];
    !reserved.contains(&lower.as_str())
        && bare.chars().next().map(|c| c.is_alphabetic() || c == '_').unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_new_assignment() {
        let src = "<?php\n$user = new User();\n$user->";
        assert_eq!(infer_var_type(src, "$user").as_deref(), Some("User"));
    }

    #[test]
    fn infers_typed_param() {
        let src = "<?php\nfunction handle(PaymentGateway $gateway) {\n    $gateway->\n}";
        assert_eq!(infer_var_type(src, "$gateway").as_deref(), Some("PaymentGateway"));
    }

    #[test]
    fn ignores_non_type_modifiers() {
        // `return $result` must not treat `return` as a type
        let src = "<?php\n$x = compute();\nreturn $x;";
        assert_eq!(infer_var_type(src, "$x"), None);
    }
}
