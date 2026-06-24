//! PHPDoc type lattice (docs/19 — Phase E1).
//!
//! Pure parsing of `/** … */` docblocks into the class-like types Photon's
//! navigation/completion can use: `@return`, `@var`, `@property[-read|-write]`,
//! and `@method`. Handles nullable (`?T`), unions (`A|B`), generics
//! (`Collection<User>`), and array shapes (`User[]`). Scalars/builtins are
//! dropped — only navigable class types are returned.

/// Structured types pulled from one docblock. All values are short class names.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct DocTypes {
    pub ret: Option<String>,
    pub var: Option<String>,
    /// Generic element of `@return`/`@var` (`Collection<User>` → `User`).
    pub ret_item: Option<String>,
    pub var_item: Option<String>,
    /// `@property[-read|-write] T $name` → (name, type).
    pub properties: Vec<(String, String)>,
    /// Generic element per property (`@property Collection<Order> $orders`).
    pub prop_items: Vec<(String, String)>,
    /// `@method [static] T name(...)` → (name, return type).
    pub methods: Vec<(String, String)>,
    /// Generic element per method return.
    pub method_items: Vec<(String, String)>,
    /// `@mixin ClassName` — this class delegates to the named class's methods.
    pub mixins: Vec<String>,
}

const BUILTINS: &[&str] = &[
    "int", "integer", "float", "double", "string", "bool", "boolean", "array",
    "void", "mixed", "iterable", "object", "callable", "null", "never", "true",
    "false", "scalar", "resource", "list",
];

fn is_builtin(t: &str) -> bool {
    BUILTINS.contains(&t.to_ascii_lowercase().as_str())
}

/// Normalize a raw type expression to a single navigable short class name.
/// Returns `None` for scalars/builtins/empty. `self`/`static`/`$this`/`parent`
/// are preserved (the chain resolver understands them).
pub fn normalize_type(raw: &str) -> Option<String> {
    let mut t = raw.trim();
    t = t.trim_start_matches('?').trim();
    // First union/intersection member that isn't null.
    for part in t.split(['|', '&']) {
        let p = part.trim().trim_start_matches('?').trim();
        if p.is_empty() || p.eq_ignore_ascii_case("null") {
            continue;
        }
        // Generic base: `Collection<User>` → `Collection`.
        let base = p.split('<').next().unwrap_or(p).trim();
        // Array shape `User[]` is an array, not navigable as the element type.
        if base.ends_with("[]") || is_builtin(base) {
            continue;
        }
        let base = base.trim_start_matches('\\');
        let short = base.rsplit('\\').next().unwrap_or(base).trim();
        if short.is_empty() {
            continue;
        }
        // Guard against stray punctuation.
        if short.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '$') {
            return Some(short.to_string());
        }
    }
    None
}

/// The element/inner type of a generic or array: `Collection<User>` → `User`,
/// `array<int,Order>` → `Order`, `User[]` → `User`. For later chain work.
pub fn generic_inner(raw: &str) -> Option<String> {
    let t = raw.trim().trim_start_matches('?').trim();
    if let (Some(lt), Some(gt)) = (t.find('<'), t.rfind('>')) {
        if gt > lt {
            // Last generic argument (value type for maps).
            let inner = &t[lt + 1..gt];
            let last = inner.split(',').next_back().unwrap_or(inner);
            return normalize_type(last);
        }
    }
    if let Some(stripped) = t.split('|').map(str::trim).find_map(|p| p.strip_suffix("[]")) {
        return normalize_type(stripped);
    }
    None
}

/// First human description line of a docblock (skips `@tags` and decorations).
pub fn description(doc: &str) -> Option<String> {
    for raw in doc.lines() {
        let l = raw
            .trim()
            .trim_start_matches("/**")
            .trim_start_matches("*/")
            .trim_start_matches('*')
            .trim();
        if l.is_empty() || l.starts_with('@') || l == "/" {
            continue;
        }
        return Some(l.to_string());
    }
    None
}

/// The raw `@return` type expression as written (keeps generics/unions), e.g.
/// `list<array<string, mixed>>`. Stops at the first top-level space.
pub fn raw_return(doc: &str) -> Option<String> {
    for raw in doc.lines() {
        let l = raw.trim().trim_start_matches('*').trim();
        if let Some(rest) = l.strip_prefix("@return") {
            let rest = rest.trim();
            let mut depth = 0i32;
            let mut out = String::new();
            for ch in rest.chars() {
                match ch {
                    '<' | '(' | '{' => depth += 1,
                    '>' | ')' | '}' => depth -= 1,
                    c if c.is_whitespace() && depth <= 0 => break,
                    _ => {}
                }
                out.push(ch);
            }
            if !out.is_empty() {
                return Some(out);
            }
        }
    }
    None
}

/// Parse a docblock body into structured types.
pub fn parse(doc: &str) -> DocTypes {
    let mut out = DocTypes::default();
    for raw_line in doc.lines() {
        let line = raw_line.trim().trim_start_matches('*').trim();
        let rest = match line.strip_prefix('@') {
            Some(r) => r,
            None => continue,
        };
        let mut it = rest.split_whitespace();
        let tag = it.next().unwrap_or("");
        match tag {
            "return" => {
                if let Some(raw) = it.next() {
                    if let Some(ty) = normalize_type(raw) {
                        out.ret.get_or_insert(ty);
                    }
                    if let Some(inner) = generic_inner(raw) {
                        out.ret_item.get_or_insert(inner);
                    }
                }
            }
            "var" => {
                if let Some(raw) = it.next() {
                    if let Some(ty) = normalize_type(raw) {
                        out.var.get_or_insert(ty);
                    }
                    if let Some(inner) = generic_inner(raw) {
                        out.var_item.get_or_insert(inner);
                    }
                }
            }
            "property" | "property-read" | "property-write" => {
                // `@property Type $name`  (type and name may be in either order
                // for malformed docs, but Type first is the PSR convention).
                let toks: Vec<&str> = it.collect();
                let raw = toks.iter().find(|t| !t.starts_with('$')).copied();
                let name = toks
                    .iter()
                    .find(|t| t.starts_with('$'))
                    .map(|t| t.trim_start_matches('$').to_string());
                if let (Some(raw), Some(name)) = (raw, name) {
                    if let Some(ty) = normalize_type(raw) {
                        out.properties.push((name.clone(), ty));
                    }
                    if let Some(inner) = generic_inner(raw) {
                        out.prop_items.push((name, inner));
                    }
                }
            }
            "method" => {
                // `@method [static] ReturnType name(args)`
                let toks: Vec<&str> = it.collect();
                let mut idx = 0;
                if toks.get(idx) == Some(&"static") {
                    idx += 1;
                }
                // Find the token containing `(` → the method name; the token
                // before it (if any) is the return type.
                let name_pos = toks.iter().position(|t| t.contains('('));
                if let Some(np) = name_pos {
                    let name = toks[np].split('(').next().unwrap_or("").to_string();
                    let raw = if np > idx { Some(toks[np - 1]) } else { None };
                    let ret = raw.and_then(normalize_type);
                    if !name.is_empty() {
                        out.methods.push((name.clone(), ret.unwrap_or_else(|| "static".to_string())));
                        if let Some(inner) = raw.and_then(generic_inner) {
                            out.method_items.push((name, inner));
                        }
                    }
                }
            }
            "mixin" => {
                if let Some(raw) = it.next() {
                    let s = raw.trim_start_matches('\\');
                    let short = s.rsplit('\\').next().unwrap_or(s);
                    if !short.is_empty() && short.chars().all(|c| c.is_alphanumeric() || c == '_') {
                        out.mixins.push(short.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_types() {
        assert_eq!(normalize_type("User"), Some("User".into()));
        assert_eq!(normalize_type("?App\\Models\\User"), Some("User".into()));
        assert_eq!(normalize_type("Collection<User>"), Some("Collection".into()));
        assert_eq!(normalize_type("User|null"), Some("User".into()));
        assert_eq!(normalize_type("null|Order"), Some("Order".into()));
        assert_eq!(normalize_type("int"), None);
        assert_eq!(normalize_type("string|int"), None);
        assert_eq!(normalize_type("User[]"), None); // array, not the element
        assert_eq!(normalize_type("self"), Some("self".into()));
    }

    #[test]
    fn extracts_generic_inner() {
        assert_eq!(generic_inner("Collection<User>"), Some("User".into()));
        assert_eq!(generic_inner("array<int, Order>"), Some("Order".into()));
        assert_eq!(generic_inner("Post[]"), Some("Post".into()));
        assert_eq!(generic_inner("User"), None);
    }

    #[test]
    fn parses_docblock() {
        let doc = r#"/**
 * @property int $id
 * @property-read Collection<Order> $orders
 * @property \App\Models\Team $team
 * @method static Builder where(string $col, mixed $val)
 * @return self
 */"#;
        let d = parse(doc);
        assert_eq!(d.ret.as_deref(), Some("self"));
        // scalar @property int $id is dropped (not navigable)
        assert!(d.properties.iter().all(|(n, _)| n != "id"));
        assert!(d.properties.contains(&("orders".to_string(), "Collection".to_string())));
        assert!(d.properties.contains(&("team".to_string(), "Team".to_string())));
        assert!(d.methods.iter().any(|(n, t)| n == "where" && t == "Builder"));
    }

    #[test]
    fn captures_generic_element_types() {
        let doc = "/**\n * @property-read Collection<Order> $orders\n * @return Collection<User>\n */";
        let d = parse(doc);
        assert!(d.prop_items.contains(&("orders".to_string(), "Order".to_string())));
        assert_eq!(d.ret_item.as_deref(), Some("User"));
    }
}
