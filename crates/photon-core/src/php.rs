//! PHP analysis (MVP slice of `docs/05-php-analysis-engine.md`).
//!
//! Uses tree-sitter to parse PHP and extract a symbol table: namespaces,
//! classes, interfaces, traits, enums, enum cases, functions, methods,
//! properties, and class constants — with fully-qualified names and precise
//! source locations. Error-tolerant: incomplete/invalid code still yields
//! whatever symbols can be recovered.

use crate::types::{Diagnostic, RefKind, Reference, Symbol, SymbolKind};
use tree_sitter::{Node, Parser};

/// Parse a PHP source string and return its symbols.
pub fn extract_symbols(file: &str, source: &str) -> Vec<Symbol> {
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_php::LANGUAGE_PHP.into()).is_err() {
        return Vec::new();
    }
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Vec::new(),
    };
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut namespace: Option<String> = None;
    walk(tree.root_node(), bytes, file, &mut namespace, None, &mut out);
    out
}

fn walk(
    node: Node,
    src: &[u8],
    file: &str,
    namespace: &mut Option<String>,
    container: Option<&str>,
    out: &mut Vec<Symbol>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "namespace_definition" => {
                if let Some(name_node) = name_node(child) {
                    let ns = node_text(name_node, src);
                    *namespace = Some(ns.clone());
                    push(
                        out,
                        Symbol {
                            name: short_name(&ns),
                            fqn: Some(ns),
                            kind: SymbolKind::Namespace,
                            file: file.to_string(),
                            container: None,
                            line: line_of(name_node),
                            name_offset: name_node.start_byte() as u32,
                            range_start: child.start_byte() as u32,
                            range_end: child.end_byte() as u32,
                        },
                    );
                }
                // Recurse to cover braced `namespace X { ... }` bodies.
                walk(child, src, file, namespace, None, out);
            }
            "class_declaration"
            | "interface_declaration"
            | "trait_declaration"
            | "enum_declaration" => {
                let kind = match child.kind() {
                    "class_declaration" => SymbolKind::Class,
                    "interface_declaration" => SymbolKind::Interface,
                    "trait_declaration" => SymbolKind::Trait,
                    _ => SymbolKind::Enum,
                };
                if let Some(name_node) = name_node(child) {
                    let name = node_text(name_node, src);
                    let fqn = qualify(namespace.as_deref(), &name);
                    push(
                        out,
                        Symbol {
                            name: name.clone(),
                            fqn: Some(fqn),
                            kind,
                            file: file.to_string(),
                            container: None,
                            line: line_of(name_node),
                            name_offset: name_node.start_byte() as u32,
                            range_start: child.start_byte() as u32,
                            range_end: child.end_byte() as u32,
                        },
                    );
                    // Descend into the type body to collect members.
                    walk(child, src, file, namespace, Some(&name), out);
                    continue;
                }
                walk(child, src, file, namespace, container, out);
            }
            "function_definition" => {
                if let Some(name_node) = name_node(child) {
                    let name = node_text(name_node, src);
                    push(
                        out,
                        Symbol {
                            name: name.clone(),
                            fqn: Some(qualify(namespace.as_deref(), &name)),
                            kind: SymbolKind::Function,
                            file: file.to_string(),
                            container: None,
                            line: line_of(name_node),
                            name_offset: name_node.start_byte() as u32,
                            range_start: child.start_byte() as u32,
                            range_end: child.end_byte() as u32,
                        },
                    );
                }
            }
            "method_declaration" => {
                if let Some(name_node) = name_node(child) {
                    let name = node_text(name_node, src);
                    let fqn = container.map(|c| {
                        format!("{}::{}", qualify(namespace.as_deref(), c), name)
                    });
                    let is_ctor = name == "__construct";
                    push(
                        out,
                        Symbol {
                            name,
                            fqn,
                            kind: SymbolKind::Method,
                            file: file.to_string(),
                            container: container.map(str::to_string),
                            line: line_of(name_node),
                            name_offset: name_node.start_byte() as u32,
                            range_start: child.start_byte() as u32,
                            range_end: child.end_byte() as u32,
                        },
                    );
                    // PHP 8 constructor property promotion: a parameter with a
                    // visibility modifier (public/protected/private/readonly)
                    // declares a real class property. Index those so they show
                    // up in completion and undefined-member checks.
                    if is_ctor {
                        extract_promoted(child, src, file, container, out);
                    }
                }
            }
            "property_declaration" => {
                collect_named(child, "variable_name", src, |name, n| {
                    push(
                        out,
                        Symbol {
                            name: name.trim_start_matches('$').to_string(),
                            fqn: None,
                            kind: SymbolKind::Property,
                            file: file.to_string(),
                            container: container.map(str::to_string),
                            line: line_of(n),
                            name_offset: n.start_byte() as u32,
                            range_start: child.start_byte() as u32,
                            range_end: child.end_byte() as u32,
                        },
                    );
                });
            }
            "const_declaration" => {
                collect_named(child, "const_element", src, |_text, n| {
                    if let Some(name_node) = name_node(n) {
                        push(
                            out,
                            Symbol {
                                name: node_text(name_node, src),
                                fqn: None,
                                kind: SymbolKind::Constant,
                                file: file.to_string(),
                                container: container.map(str::to_string),
                                line: line_of(name_node),
                                name_offset: name_node.start_byte() as u32,
                                range_start: child.start_byte() as u32,
                                range_end: child.end_byte() as u32,
                            },
                        );
                    }
                });
            }
            "enum_case" => {
                if let Some(name_node) = name_node(child) {
                    push(
                        out,
                        Symbol {
                            name: node_text(name_node, src),
                            fqn: None,
                            kind: SymbolKind::EnumCase,
                            file: file.to_string(),
                            container: container.map(str::to_string),
                            line: line_of(name_node),
                            name_offset: name_node.start_byte() as u32,
                            range_start: child.start_byte() as u32,
                            range_end: child.end_byte() as u32,
                        },
                    );
                }
            }
            _ => {
                // Keep descending to find declarations nested in blocks.
                walk(child, src, file, namespace, container, out);
            }
        }
    }
}

/// Find the identifier node for a declaration, tolerant of grammar field naming.
fn name_node(node: Node) -> Option<Node> {
    if let Some(n) = node.child_by_field_name("name") {
        return Some(n);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(child.kind(), "name" | "namespace_name") {
            return Some(child);
        }
    }
    None
}

/// Visit direct/descendant children of a given kind and hand back their text.
fn collect_named<F: FnMut(&str, Node)>(node: Node, kind: &str, src: &[u8], mut f: F) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == kind {
            f(&node_text(child, src), child);
        } else {
            // one level deeper (e.g. property_element wrapping variable_name)
            let mut inner = child.walk();
            for g in child.children(&mut inner) {
                if g.kind() == kind {
                    f(&node_text(g, src), g);
                }
            }
        }
    }
}

/// First direct child of the given kind.
fn first_child_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    let found = node.children(&mut cursor).find(|c| c.kind() == kind);
    found
}

/// Emit Property symbols for PHP 8 constructor-promoted parameters.
fn extract_promoted(
    method: Node,
    src: &[u8],
    file: &str,
    container: Option<&str>,
    out: &mut Vec<Symbol>,
) {
    let params = method
        .child_by_field_name("parameters")
        .or_else(|| first_child_kind(method, "formal_parameters"));
    let params = match params {
        Some(p) => p,
        None => return,
    };
    let mut cursor = params.walk();
    for p in params.children(&mut cursor) {
        // tree-sitter-php tags promoted params as `property_promotion_parameter`.
        // Fall back to detecting a visibility modifier for grammar robustness.
        let promoted = p.kind() == "property_promotion_parameter"
            || (p.kind().ends_with("parameter") && has_visibility_modifier(p));
        if !promoted {
            continue;
        }
        if let Some(vn) = first_child_kind(p, "variable_name") {
            push(
                out,
                Symbol {
                    name: node_text(vn, src).trim_start_matches('$').to_string(),
                    fqn: None,
                    kind: SymbolKind::Property,
                    file: file.to_string(),
                    container: container.map(str::to_string),
                    line: line_of(vn),
                    name_offset: vn.start_byte() as u32,
                    range_start: p.start_byte() as u32,
                    range_end: p.end_byte() as u32,
                },
            );
        }
    }
}

fn has_visibility_modifier(node: Node) -> bool {
    let mut cursor = node.walk();
    let found = node.children(&mut cursor).any(|c| {
        matches!(
            c.kind(),
            "visibility_modifier" | "var_modifier" | "readonly_modifier" | "abstract_modifier"
        )
    });
    found
}

fn node_text(node: Node, src: &[u8]) -> String {
    std::str::from_utf8(&src[node.start_byte()..node.end_byte()])
        .unwrap_or("")
        .to_string()
}

fn line_of(node: Node) -> u32 {
    node.start_position().row as u32 + 1
}

fn qualify(ns: Option<&str>, name: &str) -> String {
    match ns {
        Some(ns) if !ns.is_empty() => format!("{}\\{}", ns.trim_end_matches('\\'), name),
        _ => name.to_string(),
    }
}

fn short_name(fqn: &str) -> String {
    fqn.rsplit('\\').next().unwrap_or(fqn).to_string()
}

fn push(out: &mut Vec<Symbol>, s: Symbol) {
    out.push(s);
}

// ---------------------------------------------------------------------------
// Member types — declared return types of methods and types of properties.
// Powers the chain-resolving type engine (`$x->method()->prop->…`).
// ---------------------------------------------------------------------------

/// `(container, member, kind, type)` where kind ∈ {method, property} and type
/// is the (shortened) declared class/scalar. Reuses the symbol parse.
pub fn extract_member_types(file: &str, source: &str) -> Vec<(String, String, String, String)> {
    use crate::phpdoc;
    let mut out = Vec::new();
    for s in extract_symbols(file, source) {
        let start = s.range_start as usize;
        match s.kind {
            SymbolKind::Class | SymbolKind::Interface | SymbolKind::Trait => {
                // Class docblock → magic members (`@property`, `@method`).
                if let Some(doc) = preceding_docblock(source, start) {
                    let d = phpdoc::parse(doc);
                    for (name, ty) in d.properties {
                        out.push((s.name.clone(), name, "property".into(), ty));
                    }
                    for (name, ty) in d.methods {
                        out.push((s.name.clone(), name, "method".into(), ty));
                    }
                    // Generic element types (Collection<Order> → Order) for
                    // collection-aware chain resolution.
                    for (name, ty) in d.prop_items {
                        out.push((s.name.clone(), name, "property_item".into(), ty));
                    }
                    for (name, ty) in d.method_items {
                        out.push((s.name.clone(), name, "method_item".into(), ty));
                    }
                }
                continue;
            }
            SymbolKind::Method | SymbolKind::Property => {}
            _ => continue,
        }
        if s.range_end <= s.range_start {
            continue;
        }
        let container = match &s.container {
            Some(c) => c.clone(),
            None => continue,
        };
        let decl = &source[start.min(source.len())..(s.range_end as usize).min(source.len())];
        let (kind, ty) = match s.kind {
            SymbolKind::Method => ("method", parse_return_type(decl)),
            SymbolKind::Property => ("property", parse_property_type(decl, &s.name)),
            _ => continue,
        };
        // Native declared type wins; fall back to the member's own docblock
        // (`@return` / `@var`) so generics/union/`self` still resolve.
        let resolved = ty.and_then(|t| {
            let short = t.rsplit('\\').next().unwrap_or(&t).to_string();
            (!short.is_empty()).then_some(short)
        });
        let resolved = resolved.or_else(|| {
            let doc = preceding_docblock(source, start)?;
            let d = phpdoc::parse(doc);
            match s.kind {
                SymbolKind::Method => d.ret,
                _ => d.var,
            }
        });
        if let Some(short) = resolved {
            out.push((container.clone(), s.name.clone(), kind.to_string(), short));
        }
        // Element type from the member's own docblock (`@return Collection<X>`).
        if let Some(doc) = preceding_docblock(source, start) {
            let d = phpdoc::parse(doc);
            let item = match s.kind {
                SymbolKind::Method => d.ret_item,
                _ => d.var_item,
            };
            if let Some(elem) = item {
                out.push((container, s.name, format!("{kind}_item"), elem));
            }
        }
    }
    out
}

/// The `/** … */` docblock immediately preceding a byte offset, if any.
fn preceding_docblock(source: &str, range_start: usize) -> Option<&str> {
    let head = &source[..range_start.min(source.len())];
    let trimmed = head.trim_end();
    if !trimmed.ends_with("*/") {
        return None;
    }
    let open = trimmed.rfind("/**")?;
    // Reject if there's a statement between the docblock and the symbol.
    Some(&trimmed[open..])
}

// ---------------------------------------------------------------------------
// Structural inspections (docs/19 — Phase E2): unreachable code after a jump,
// and missing return type on functions that return a value. Tree-sitter based
// for accuracy; high-signal and conservative.
// ---------------------------------------------------------------------------

type EnumMap = std::collections::HashMap<String, std::collections::HashSet<String>>;

/// Diagnostics from structural analysis of one PHP file.
pub fn analysis_diagnostics(source: &str) -> Vec<Diagnostic> {
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_php::LANGUAGE_PHP.into()).is_err() {
        return Vec::new();
    }
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Vec::new(),
    };
    let bytes = source.as_bytes();
    let mut enums: EnumMap = std::collections::HashMap::new();
    collect_enums(tree.root_node(), bytes, &mut enums);
    let mut out = Vec::new();
    walk_analysis(tree.root_node(), bytes, &enums, &mut out);
    out
}

fn collect_enums(node: Node, src: &[u8], out: &mut EnumMap) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "enum_declaration" {
            if let Some(nm) = name_node(child) {
                let mut cases = std::collections::HashSet::new();
                for d in descendants(child) {
                    if d.kind() == "enum_case" {
                        if let Some(cn) = name_node(d) {
                            cases.insert(node_text(cn, src));
                        }
                    }
                }
                out.insert(node_text(nm, src), cases);
            }
        }
        collect_enums(child, src, out);
    }
}

fn walk_analysis(node: Node, src: &[u8], enums: &EnumMap, out: &mut Vec<Diagnostic>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "function_definition" | "method_declaration" => check_missing_return(child, src, out),
            "compound_statement" => check_unreachable(child, out),
            "class_declaration" => {
                check_readonly(child, src, out);
                check_unused_private(child, src, out);
            }
            "match_expression" => check_match_exhaustive(child, src, enums, out),
            _ => {}
        }
        walk_analysis(child, src, enums, out);
    }
}

/// Warn when a `match` over a known enum omits cases and has no `default`.
fn check_match_exhaustive(node: Node, src: &[u8], enums: &EnumMap, out: &mut Vec<Diagnostic>) {
    use std::collections::{HashMap, HashSet};
    // Gather `Enum::Case` references inside the match, grouped by enum scope.
    let mut by_scope: HashMap<String, HashSet<String>> = HashMap::new();
    for d in descendants(node) {
        if d.kind() == "class_constant_access_expression" {
            let txt = node_text(d, src);
            if let Some((scope, konst)) = txt.split_once("::") {
                let scope = scope.trim().trim_start_matches('\\').rsplit('\\').next().unwrap_or("").to_string();
                by_scope.entry(scope).or_default().insert(konst.trim().to_string());
            }
        }
    }
    // Exactly one referenced scope that is a known enum → analyzable.
    let known: Vec<&String> = by_scope.keys().filter(|s| enums.contains_key(*s)).collect();
    if known.len() != 1 {
        return;
    }
    let scope = known[0].clone();
    if match_has_default(node, src) {
        return;
    }
    let all = &enums[&scope];
    let covered = &by_scope[&scope];
    let missing: Vec<&String> = all.iter().filter(|c| !covered.contains(*c)).collect();
    if missing.is_empty() {
        return;
    }
    let mut names: Vec<&str> = missing.iter().map(|s| s.as_str()).collect();
    names.sort_unstable();
    let p = node.start_position();
    out.push(Diagnostic {
        line: p.row as u32 + 1,
        col: p.column as u32 + 1,
        end_col: p.column as u32 + 6, // the `match` keyword
        message: format!(
            "match on enum {} is not exhaustive — missing: {}",
            scope,
            names.join(", ")
        ),
        severity: "warning".into(),
    });
}

fn match_has_default(node: Node, src: &[u8]) -> bool {
    for d in descendants(node) {
        if d.kind() == "match_default_expression" {
            return true;
        }
    }
    let collapsed: String = node_text(node, src).split_whitespace().collect();
    collapsed.contains("default=>")
}

/// Flag private methods/properties never referenced within their class.
fn check_unused_private(class_node: Node, src: &[u8], out: &mut Vec<Diagnostic>) {
    let class_text = node_text(class_node, src);
    for n in descendants(class_node) {
        match n.kind() {
            "method_declaration" => {
                let txt = node_text(n, src);
                let head = &txt[..txt.find('(').unwrap_or(txt.len())];
                if !head.contains("private") {
                    continue;
                }
                if let Some(nm) = name_node(n) {
                    let name = node_text(nm, src);
                    if name.starts_with("__") || member_referenced(&class_text, &name) {
                        continue;
                    }
                    out.push(unused_diag(nm, &name, "method"));
                }
            }
            "property_declaration" => {
                let txt = node_text(n, src);
                let head = &txt[..txt.find('$').unwrap_or(txt.len())];
                if !head.contains("private") {
                    continue;
                }
                collect_named(n, "variable_name", src, |raw, vn| {
                    let name = raw.trim_start_matches('$').to_string();
                    if !member_referenced(&class_text, &name) {
                        out.push(unused_diag(vn, &name, "property"));
                    }
                });
            }
            _ => {}
        }
    }
}

fn unused_diag(name_node: Node, name: &str, kind: &str) -> Diagnostic {
    let p = name_node.start_position();
    Diagnostic {
        line: p.row as u32 + 1,
        col: p.column as u32 + 1,
        end_col: p.column as u32 + 1 + name.chars().count() as u32,
        message: format!("Unused private {} '{}'", kind, name),
        severity: "warning".into(),
    }
}

/// True if `->name` or `::name` appears in `text` as a member access.
fn member_referenced(text: &str, name: &str) -> bool {
    let bytes = text.as_bytes();
    for sep in ["->", "::"] {
        let needle = format!("{sep}{name}");
        let mut i = 0;
        while let Some(p) = text[i..].find(&needle) {
            let at = i + p;
            i = at + needle.len();
            let after = bytes.get(at + needle.len()).copied();
            let ok = after.map(|b| !(b.is_ascii_alphanumeric() || b == b'_')).unwrap_or(true);
            if ok {
                return true;
            }
        }
    }
    false
}

fn diag_at(node: Node, message: String, severity: &str) -> Diagnostic {
    let p = node.start_position();
    let e = node.end_position();
    let col = p.column as u32 + 1;
    let end_col = if e.row == p.row {
        e.column as u32 + 1
    } else {
        col + 1
    };
    Diagnostic {
        line: p.row as u32 + 1,
        col,
        end_col,
        message,
        severity: severity.to_string(),
    }
}

fn is_terminator(node: Node) -> bool {
    match node.kind() {
        "return_statement" | "break_statement" | "continue_statement" | "goto_statement"
        | "throw_statement" => true,
        "expression_statement" => {
            let mut c = node.walk();
            let found = node
                .children(&mut c)
                .any(|g| matches!(g.kind(), "throw_expression" | "exit_intrinsic" | "exit_statement"));
            found
        }
        _ => false,
    }
}

/// Flag the first statement that follows an unconditional jump in a block.
fn check_unreachable(block: Node, out: &mut Vec<Diagnostic>) {
    let mut cursor = block.walk();
    let mut terminated = false;
    for c in block.children(&mut cursor) {
        if !c.is_named() || c.kind() == "comment" {
            continue;
        }
        if terminated {
            out.push(diag_at(c, "Unreachable code".to_string(), "warning"));
            return; // one flag per block keeps it calm
        }
        if is_terminator(c) {
            terminated = true;
        }
    }
}

/// True if the body returns a value (`return $x;`) without descending into
/// nested closures/functions (whose returns belong to them).
fn body_returns_value(node: Node) -> bool {
    let mut cursor = node.walk();
    for c in node.children(&mut cursor) {
        match c.kind() {
            "anonymous_function_creation_expression"
            | "arrow_function"
            | "function_definition"
            | "method_declaration" => continue,
            "return_statement" => {
                if c.named_child_count() > 0 {
                    return true;
                }
            }
            _ => {}
        }
        if body_returns_value(c) {
            return true;
        }
    }
    false
}

/// Flag re-assignment of a `readonly` property outside the constructor (PHP
/// 8.1/8.2). Conservative: only `$this->prop` within the declaring class.
fn check_readonly(class_node: Node, src: &[u8], out: &mut Vec<Diagnostic>) {
    use std::collections::HashSet;
    let class_text = node_text(class_node, src);
    let head = &class_text[..class_text.find("class").unwrap_or(0)];
    let readonly_class = head.contains("readonly");

    let mut ro: HashSet<String> = HashSet::new();
    collect_readonly_props(class_node, src, readonly_class, &mut ro);
    if ro.is_empty() {
        return;
    }

    // Search each non-constructor method body for `$this->prop = …`.
    for n in descendants(class_node) {
        if n.kind() != "method_declaration" && n.kind() != "function_definition" {
            continue;
        }
        let mname = name_node(n).map(|x| node_text(x, src)).unwrap_or_default();
        if mname == "__construct" {
            continue;
        }
        if let Some(body) = n
            .child_by_field_name("body")
            .or_else(|| first_child_kind(n, "compound_statement"))
        {
            readonly_assignments(body, src, &ro, out);
        }
    }
}

fn collect_readonly_props(
    node: Node,
    src: &[u8],
    readonly_class: bool,
    out: &mut std::collections::HashSet<String>,
) {
    for n in descendants(node) {
        match n.kind() {
            "property_declaration" => {
                let txt = node_text(n, src);
                if readonly_class || txt.contains("readonly") {
                    collect_named(n, "variable_name", src, |name, _| {
                        out.insert(name.trim_start_matches('$').to_string());
                    });
                }
            }
            "property_promotion_parameter" => {
                let txt = node_text(n, src);
                if readonly_class || txt.contains("readonly") {
                    if let Some(vn) = first_child_kind(n, "variable_name") {
                        out.insert(node_text(vn, src).trim_start_matches('$').to_string());
                    }
                }
            }
            _ => {}
        }
    }
}

fn readonly_assignments(
    body: Node,
    src: &[u8],
    ro: &std::collections::HashSet<String>,
    out: &mut Vec<Diagnostic>,
) {
    for n in descendants(body) {
        if !matches!(n.kind(), "assignment_expression" | "augmented_assignment_expression") {
            continue;
        }
        let left = match n.child_by_field_name("left").or_else(|| n.named_child(0)) {
            Some(l) => l,
            None => continue,
        };
        let lt = node_text(left, src);
        let rest = match lt.strip_prefix("$this->") {
            Some(r) => r,
            None => continue,
        };
        let prop: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        if prop.is_empty() || !ro.contains(&prop) {
            continue;
        }
        let p = left.start_position();
        out.push(Diagnostic {
            line: p.row as u32 + 1,
            col: p.column as u32 + 1,
            end_col: p.column as u32 + 1 + ("$this->".len() + prop.len()) as u32,
            message: format!(
                "Cannot assign to readonly property '$this->{}' outside the constructor",
                prop
            ),
            severity: "error".into(),
        });
    }
}

/// All descendants of a node (excluding the node itself), depth-first, without
/// crossing into nested class declarations.
fn descendants(node: Node) -> Vec<Node> {
    let mut out = Vec::new();
    let mut cursor = node.walk();
    for c in node.children(&mut cursor) {
        out.push(c);
        if c.kind() != "class_declaration" {
            out.extend(descendants(c));
        }
    }
    out
}

fn check_missing_return(fnode: Node, src: &[u8], out: &mut Vec<Diagnostic>) {
    let name_node = match name_node(fnode) {
        Some(n) => n,
        None => return,
    };
    let name = node_text(name_node, src);
    if matches!(name.as_str(), "__construct" | "__destruct") {
        return;
    }
    let body = match fnode
        .child_by_field_name("body")
        .or_else(|| first_child_kind(fnode, "compound_statement"))
    {
        Some(b) => b,
        None => return, // abstract / interface method — no body to analyze
    };
    // Authoritative, grammar-independent check: a `:` after the closing paren of
    // the signature means a return type is declared.
    let sig = std::str::from_utf8(&src[fnode.start_byte()..body.start_byte()]).unwrap_or("");
    let has_return_type = sig.rfind(')').map(|rp| sig[rp..].contains(':')).unwrap_or(false);
    if has_return_type || !body_returns_value(body) {
        return;
    }
    let p = name_node.start_position();
    out.push(Diagnostic {
        line: p.row as u32 + 1,
        col: p.column as u32 + 1,
        end_col: p.column as u32 + 1 + name.chars().count() as u32,
        message: format!("Missing return type on '{}()'", name),
        severity: "warning".into(),
    });
}

/// Parameter names (without `$`) from a declaration — named-argument completion.
pub fn param_names(decl: &str) -> Vec<String> {
    param_specs(decl).into_iter().map(|(_, n)| n).collect()
}

const PARAM_MODIFIERS: &[&str] = &["public", "private", "protected", "readonly", "static", "var"];

/// `(type, name)` for each parameter (type may be empty). Powers signature
/// rendering in the hover popup.
pub fn param_specs(decl: &str) -> Vec<(String, String)> {
    let open = match decl.find('(') {
        Some(o) => o,
        None => return Vec::new(),
    };
    let mut depth = 0i32;
    let mut close = decl.len();
    for (i, ch) in decl[open..].char_indices() {
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
    let params = &decl[open + 1..close.min(decl.len())];
    let mut out = Vec::new();
    let mut nest = 0i32;
    let mut cur = String::new();
    let flush = |seg: &str, out: &mut Vec<(String, String)>| {
        if let Some(d) = seg.find('$') {
            let name: String = seg[d + 1..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if name.is_empty() {
                return;
            }
            let ty = seg[..d]
                .split_whitespace()
                .filter(|t| !PARAM_MODIFIERS.contains(t))
                .collect::<Vec<_>>()
                .join(" ");
            out.push((ty.trim().to_string(), name));
        }
    };
    for ch in params.chars() {
        match ch {
            '(' | '[' | '{' | '<' => {
                nest += 1;
                cur.push(ch);
            }
            ')' | ']' | '}' | '>' => {
                nest -= 1;
                cur.push(ch);
            }
            ',' if nest == 0 => {
                flush(&cur, &mut out);
                cur.clear();
            }
            _ => cur.push(ch),
        }
    }
    flush(&cur, &mut out);
    out
}

/// Public wrapper for the native return type parsed from a declaration.
pub fn return_type(decl: &str) -> Option<String> {
    parse_return_type(decl)
}

/// Public wrapper: the docblock immediately preceding a byte offset.
pub fn doc_before(source: &str, offset: usize) -> Option<&str> {
    preceding_docblock(source, offset)
}

/// Convert a (possibly generic) docblock type into a valid native PHP type:
/// `Collection<User>` → `Collection`, `User[]`/`array<…>`/`list<…>` → `array`,
/// `?App\Models\User` → `?User`. Returns `None` for unrecognized shapes.
pub fn native_type(raw: &str) -> Option<String> {
    let t = raw.trim();
    if t.is_empty() {
        return None;
    }
    let low = t.to_ascii_lowercase();
    if t.ends_with("[]")
        || low.starts_with("array<")
        || low.starts_with("list<")
        || low.starts_with("iterable<")
        || low == "array"
        || low == "list"
    {
        return Some("array".into());
    }
    let nullable = t.starts_with('?');
    let base = t.trim_start_matches('?').split('<').next().unwrap_or(t).trim();
    let lb = base.to_ascii_lowercase();
    if [
        "int", "float", "string", "bool", "void", "mixed", "object", "callable",
        "iterable", "never", "self", "static", "parent", "false", "true", "null",
    ]
    .contains(&lb.as_str())
    {
        return Some(base.to_string());
    }
    let short = base.trim_start_matches('\\').rsplit('\\').next().unwrap_or(base);
    if short.chars().next().map(|c| c.is_ascii_uppercase()).unwrap_or(false) {
        return Some(if nullable { format!("?{short}") } else { short.to_string() });
    }
    None
}

/// What the "add return type" quick-fix needs: where to insert, a confident
/// literal/doc type if we have one, and otherwise the first return expression
/// (as a chain + offset) for the caller to resolve via the type engine.
pub struct ReturnSuggestion {
    pub insert_line: u32,
    pub insert_col: u32,
    pub literal: Option<String>,
    pub chain: Option<(String, u32)>,
}

/// Analyze the function/method whose name is on `target_line`. Returns `None` if
/// it already declares a return type or can't be located. Never guesses
/// `mixed` — an unresolvable return yields neither a literal nor (if not a
/// chain) anything, so the caller can decline to offer a fix.
pub fn analyze_return(source: &str, target_line: u32) -> Option<ReturnSuggestion> {
    let mut parser = Parser::new();
    parser.set_language(&tree_sitter_php::LANGUAGE_PHP.into()).ok()?;
    let tree = parser.parse(source, None)?;
    let bytes = source.as_bytes();
    let node = find_fn_on_line(tree.root_node(), target_line)?;
    let body = node
        .child_by_field_name("body")
        .or_else(|| first_child_kind(node, "compound_statement"))?;
    let sig = std::str::from_utf8(&bytes[node.start_byte()..body.start_byte()]).unwrap_or("");
    if sig.rfind(')').map(|r| sig[r..].contains(':')).unwrap_or(false) {
        return None; // already typed
    }
    let params = node
        .child_by_field_name("parameters")
        .or_else(|| first_child_kind(node, "formal_parameters"))?;
    let ep = params.end_position();
    let doc_native = doc_before(source, node.start_byte())
        .and_then(crate::phpdoc::raw_return)
        .and_then(|raw| native_type(&raw));
    let first = find_first_value_return(body);
    let literal = doc_native.or_else(|| first.and_then(|e| classify_literal(e, bytes)));
    let chain = if literal.is_some() {
        None
    } else {
        first.and_then(|e| chain_of(e, bytes))
    };
    Some(ReturnSuggestion {
        insert_line: ep.row as u32 + 1,
        insert_col: ep.column as u32 + 1,
        literal,
        chain,
    })
}

fn find_fn_on_line(node: Node, target_line: u32) -> Option<Node> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(child.kind(), "function_definition" | "method_declaration") {
            if let Some(nm) = name_node(child) {
                if nm.start_position().row as u32 + 1 == target_line {
                    return Some(child);
                }
            }
        }
        if let Some(found) = find_fn_on_line(child, target_line) {
            return Some(found);
        }
    }
    None
}

/// First value-returning `return <expr>;` expression, not descending into
/// nested closures/functions (whose returns are their own).
fn find_first_value_return(node: Node) -> Option<Node> {
    let mut cursor = node.walk();
    for c in node.children(&mut cursor) {
        match c.kind() {
            "anonymous_function_creation_expression"
            | "arrow_function"
            | "function_definition"
            | "method_declaration" => continue,
            "return_statement" if c.named_child_count() > 0 => return c.named_child(0),
            _ => {}
        }
        if let Some(found) = find_first_value_return(c) {
            return Some(found);
        }
    }
    None
}

/// A confident native type for a literal return expression (no `mixed`).
fn classify_literal(e: Node, src: &[u8]) -> Option<String> {
    let t = match e.kind() {
        "array_creation_expression" => "array".to_string(),
        "object_creation_expression" => first_child_kind(e, "name")
            .or_else(|| first_child_kind(e, "qualified_name"))
            .map(|n| {
                let t = node_text(n, src);
                t.rsplit('\\').next().unwrap_or(&t).to_string()
            })
            .unwrap_or_else(|| "object".to_string()),
        "string" | "encapsed_string" | "heredoc" => "string".to_string(),
        "integer" => "int".to_string(),
        "float" => "float".to_string(),
        "boolean" | "true" | "false" => "bool".to_string(),
        "null" => "null".to_string(),
        "variable_name" if node_text(e, src) == "$this" => "self".to_string(),
        _ => return None,
    };
    Some(t)
}

/// A resolvable receiver chain (`$var`, `$this->svc->find()`, `Foo::make()`)
/// for the type engine to resolve, plus its byte offset.
fn chain_of(e: Node, src: &[u8]) -> Option<(String, u32)> {
    if matches!(
        e.kind(),
        "variable_name"
            | "member_access_expression"
            | "member_call_expression"
            | "scoped_call_expression"
            | "scoped_property_access_expression"
            | "nullsafe_member_access_expression"
            | "nullsafe_member_call_expression"
            | "function_call_expression"
    ) {
        let text = node_text(e, src);
        if text == "$this" {
            return None;
        }
        return Some((text, e.start_byte() as u32));
    }
    None
}

fn parse_return_type(decl: &str) -> Option<String> {
    let body = decl.find('{').map(|i| &decl[..i]).unwrap_or(decl);
    let close = body.rfind(')')?;
    let rest = body[close + 1..].trim_start().strip_prefix(':')?.trim_start();
    let raw: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '\\' || *c == '_' || *c == '?' || *c == '|' || *c == '&')
        .collect();
    let first = raw
        .trim_start_matches('?')
        .split(['|', '&'])
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    if first.is_empty() {
        None
    } else {
        Some(first)
    }
}

fn parse_property_type(decl: &str, name: &str) -> Option<String> {
    let idx = decl.find(&format!("${}", name))?;
    let before = decl[..idx].trim_end();
    let tok = before
        .rsplit(|c: char| c == ' ' || c == '\t' || c == '(' || c == ',')
        .find(|s| !s.is_empty())?;
    let tok = tok.trim_start_matches('?');
    let modifier = matches!(
        tok,
        "public" | "private" | "protected" | "static" | "readonly" | "var" | "const"
    );
    if modifier || tok.is_empty() {
        return None;
    }
    let first = tok.split(['|', '&']).next().unwrap_or("").trim().to_string();
    if first.is_empty() {
        None
    } else {
        Some(first)
    }
}

// ---------------------------------------------------------------------------
// Type relations (extends / implements / uses) — powers Go-to-Implementation
// and interface/trait usages (docs/02 §navigation).
// ---------------------------------------------------------------------------

/// `(source_type, target_type, rel)` where rel ∈ {extends, implements, uses}.
pub fn extract_type_relations(file: &str, source: &str) -> Vec<(String, String, String)> {
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_php::LANGUAGE_PHP.into()).is_err() {
        return Vec::new();
    }
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Vec::new(),
    };
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let _ = file;
    walk_relations(tree.root_node(), bytes, &mut out);
    out
}

fn walk_relations(node: Node, src: &[u8], out: &mut Vec<(String, String, String)>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(
            child.kind(),
            "class_declaration" | "interface_declaration" | "trait_declaration" | "enum_declaration"
        ) {
            if let Some(name_node) = name_node(child) {
                let src_name = node_text(name_node, src);
                let mut c2 = child.walk();
                for part in child.children(&mut c2) {
                    match part.kind() {
                        "base_clause" => {
                            for n in type_names_in(part) {
                                out.push((src_name.clone(), node_text(n, src), "extends".into()));
                            }
                        }
                        "class_interface_clause" => {
                            for n in type_names_in(part) {
                                out.push((src_name.clone(), node_text(n, src), "implements".into()));
                            }
                        }
                        "declaration_list" => {
                            // trait usage: `use TraitA, TraitB;` inside the body
                            let mut c3 = part.walk();
                            for member in part.children(&mut c3) {
                                if member.kind() == "use_declaration" {
                                    for n in type_names_in(member) {
                                        out.push((src_name.clone(), node_text(n, src), "uses".into()));
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        walk_relations(child, src, out);
    }
}

/// Last-segment `name` nodes for each name/qualified_name under a node.
fn type_names_in(node: Node) -> Vec<Node> {
    let mut out = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(child.kind(), "name" | "qualified_name") {
            if let Some(n) = last_name(child) {
                out.push(n);
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Reference extraction (use-sites) — powers Find Usages, x-file go-to-def,
// and Rename. See docs/02-module-design.md §navigation/refactoring.
// ---------------------------------------------------------------------------

/// Parse PHP and return every reference (use-site of a name) we can recover.
pub fn extract_references(file: &str, source: &str) -> Vec<Reference> {
    let mut parser = Parser::new();
    if parser.set_language(&tree_sitter_php::LANGUAGE_PHP.into()).is_err() {
        return Vec::new();
    }
    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Vec::new(),
    };
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    walk_refs(tree.root_node(), bytes, file, &mut out);
    out
}

fn walk_refs(node: Node, src: &[u8], file: &str, out: &mut Vec<Reference>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "object_creation_expression" => {
                if let Some(n) = type_name_node(child) {
                    add_ref(out, n, src, file, RefKind::TypeRef);
                }
            }
            "base_clause" | "class_interface_clause" => {
                // extends / implements — may list several names.
                let mut c = child.walk();
                for g in child.children(&mut c) {
                    if matches!(g.kind(), "name" | "qualified_name") {
                        if let Some(n) = last_name(g) {
                            add_ref(out, n, src, file, RefKind::TypeRef);
                        }
                    }
                }
            }
            "scoped_call_expression"
            | "class_constant_access_expression"
            | "scoped_property_access_expression" => {
                // Foo::bar(), Foo::CONST, Foo::$prop
                if let Some(scope) = child.child(0) {
                    if matches!(scope.kind(), "name" | "qualified_name") {
                        if let Some(n) = last_name(scope) {
                            add_ref(out, n, src, file, RefKind::StaticRef);
                        }
                    }
                }
            }
            "function_call_expression" => {
                if let Some(callee) = child.child_by_field_name("function").or_else(|| child.child(0))
                {
                    if matches!(callee.kind(), "name" | "qualified_name") {
                        if let Some(n) = last_name(callee) {
                            add_ref(out, n, src, file, RefKind::Call);
                        }
                    }
                }
                // recurse into arguments for nested calls/types
                walk_refs(child, src, file, out);
                continue;
            }
            "member_call_expression" | "member_access_expression" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    if name_node.kind() == "name" {
                        add_ref(out, name_node, src, file, RefKind::Member);
                    }
                }
                walk_refs(child, src, file, out);
                continue;
            }
            "namespace_use_clause" => {
                let mut c = child.walk();
                for g in child.children(&mut c) {
                    if matches!(g.kind(), "qualified_name" | "name") {
                        if let Some(n) = last_name(g) {
                            add_ref(out, n, src, file, RefKind::Import);
                        }
                    }
                }
            }
            "named_type" => {
                if let Some(n) = type_name_node(child) {
                    add_ref(out, n, src, file, RefKind::TypeRef);
                }
            }
            _ => walk_refs(child, src, file, out),
        }
    }
}

/// First name/qualified_name within a node, reduced to its last segment.
fn type_name_node(node: Node) -> Option<Node> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(child.kind(), "name" | "qualified_name") {
            return last_name(child);
        }
    }
    None
}

/// The last `name` segment of a (possibly qualified) name node.
fn last_name(node: Node) -> Option<Node> {
    if node.kind() == "name" {
        return Some(node);
    }
    let mut last = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "name" {
            last = Some(child);
        }
    }
    last.or(Some(node))
}

fn add_ref(out: &mut Vec<Reference>, n: Node, src: &[u8], file: &str, kind: RefKind) {
    let name = node_text(n, src);
    if name.is_empty() || is_builtin(&name) {
        return;
    }
    let pos = n.start_position();
    out.push(Reference {
        name,
        kind,
        file: file.to_string(),
        line: pos.row as u32 + 1,
        column: pos.column as u32 + 1,
        start: n.start_byte() as u32,
        end: n.end_byte() as u32,
    });
}

/// Filter out built-in type/keyword names that are never user symbols.
fn is_builtin(name: &str) -> bool {
    matches!(
        name,
        "self" | "static" | "parent" | "int" | "float" | "string" | "bool" | "array"
            | "void" | "mixed" | "object" | "callable" | "iterable" | "null" | "false"
            | "true" | "never" | "parent::class"
    )
}
