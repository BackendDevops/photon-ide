//! Laravel intelligence (MVP slice of `docs/06-laravel-intelligence.md`).
//!
//! For the walking skeleton this implements route discovery from `routes/*.php`:
//! method, URI, optional `->name(...)`, and the controller/closure action.
//! It scans statements rather than booting the app (the "static source"
//! strategy from the spec); runtime reflection is a later milestone.

use crate::types::Route;

const VERBS: &[&str] = &[
    "get", "post", "put", "patch", "delete", "options", "any", "match",
    "resource", "apiResource", "view", "redirect", "fallback",
];

/// Detect whether a relative path is a Laravel routes file.
pub fn is_routes_file(rel: &str) -> bool {
    rel.starts_with("routes/") && rel.ends_with(".php")
}

/// Extract routes from a routes file's source.
pub fn extract_routes(file: &str, source: &str) -> Vec<Route> {
    let mut out = Vec::new();
    let bytes = source.as_bytes();
    let mut search_from = 0usize;

    while let Some(rel) = source[search_from..].find("Route::") {
        let start = search_from + rel;
        let after = start + "Route::".len();
        search_from = after;

        // Read the method identifier directly after `Route::`.
        let method_word = read_ident(&source[after..]);
        if method_word.is_empty() || !VERBS.contains(&method_word.as_str()) {
            continue;
        }

        // Capture the statement up to the terminating ';' (respecting nesting).
        let stmt = capture_statement(source, after);
        let line = (bytes[..start].iter().filter(|&&b| b == b'\n').count() + 1) as u32;

        let strings = string_literals(&stmt);
        // For `match([verbs], 'uri', …)` the first strings are HTTP verbs inside
        // the array — the URI is the first string AFTER that array's `]`.
        let uri = if method_word == "match" {
            let after = stmt.find(']').map(|i| &stmt[i..]).unwrap_or(stmt.as_str());
            string_literals(after).into_iter().next().unwrap_or_default()
        } else {
            strings.first().cloned().unwrap_or_default()
        };
        let name = find_named_call(&stmt, "name");
        let action = detect_action(&stmt, &strings);

        let methods = normalize_method(&method_word, &stmt);
        out.push(Route {
            method: methods,
            uri,
            name,
            action,
            file: file.to_string(),
            line,
        });
    }
    out
}

/// Read a leading identifier (letters/digits/underscore).
fn read_ident(s: &str) -> String {
    s.chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect()
}

/// Capture characters from `start` until the statement-ending ';' at paren depth 0.
fn capture_statement(source: &str, start: usize) -> String {
    let mut depth: i32 = 0;
    let mut in_str: Option<char> = None;
    let mut prev = '\0';
    let mut buf = String::new();
    for ch in source[start..].chars() {
        buf.push(ch);
        match in_str {
            Some(q) => {
                if ch == q && prev != '\\' {
                    in_str = None;
                }
            }
            None => match ch {
                '\'' | '"' => in_str = Some(ch),
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' => depth -= 1,
                ';' if depth <= 0 => break,
                _ => {}
            },
        }
        prev = ch;
        if buf.len() > 4000 {
            break; // safety bound
        }
    }
    buf
}

/// All string literals (single or double quoted) in order.
fn string_literals(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = s.char_indices().peekable();
    while let Some((_, ch)) = chars.next() {
        if ch == '\'' || ch == '"' {
            let quote = ch;
            let mut lit = String::new();
            let mut prev = '\0';
            for (_, c) in chars.by_ref() {
                if c == quote && prev != '\\' {
                    break;
                }
                lit.push(c);
                prev = c;
            }
            out.push(lit);
        }
    }
    out
}

/// Symfony-style routes declared with `#[Route(...)]` attributes on controller
/// methods. Zero-config Symfony awareness (docs/19 §framework).
pub fn extract_attribute_routes(file: &str, source: &str) -> Vec<Route> {
    let mut out = Vec::new();
    let mut i = 0usize;
    while let Some(p) = source[i..].find("#[Route(") {
        let at = i + p;
        let open = at + "#[Route".len();
        let mut depth = 0i32;
        let mut close = source.len();
        for (k, ch) in source[open..].char_indices() {
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        close = open + k;
                        break;
                    }
                }
                _ => {}
            }
        }
        let args = &source[open + 1..close.min(source.len())];
        i = close + 1;

        let strings = string_literals(args);
        let uri = strings
            .iter()
            .find(|s| s.starts_with('/'))
            .or_else(|| strings.first())
            .cloned()
            .unwrap_or_default();
        let name = args
            .find("name:")
            .and_then(|n| string_literals(&args[n + 5..]).into_iter().next());
        let method = args
            .find("methods:")
            .map(|m| {
                let rest = &args[m + 8..];
                let arr = rest.find(']').map(|x| &rest[..x]).unwrap_or(rest);
                let verbs = string_literals(arr);
                if verbs.is_empty() {
                    "ANY".to_string()
                } else {
                    verbs.iter().map(|v| v.to_uppercase()).collect::<Vec<_>>().join("|")
                }
            })
            .unwrap_or_else(|| "ANY".to_string());
        let line = (source[..at].matches('\n').count() + 1) as u32;

        let class = source[..at].rfind("class ").map(|c| {
            source[c + 6..]
                .chars()
                .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
                .collect::<String>()
        });
        let method_name = source[close..].find("function ").map(|f| {
            source[close + f + 9..]
                .trim_start()
                .chars()
                .take_while(|ch| ch.is_alphanumeric() || *ch == '_')
                .collect::<String>()
        });
        let action = match (class, method_name) {
            (Some(c), Some(m)) if !c.is_empty() && !m.is_empty() => Some(format!("{c}@{m}")),
            _ => None,
        };

        if !uri.is_empty() {
            out.push(Route { method, uri, name, action, file: file.to_string(), line });
        }
    }
    out
}

/// Find `->call('value')` and return the first string argument.
fn find_named_call(stmt: &str, call: &str) -> Option<String> {
    let needle = format!("->{}(", call);
    let idx = stmt.find(&needle)?;
    let rest = &stmt[idx + needle.len()..];
    string_literals(rest).into_iter().next()
}

/// Determine the route action: closure, `Controller@method`, or `[Controller::class, 'method']`.
fn detect_action(stmt: &str, strings: &[String]) -> Option<String> {
    if stmt.contains("function") || stmt.contains("fn(") || stmt.contains("fn ") {
        return Some("Closure".to_string());
    }
    // `'App\Http\Controllers\Foo@bar'` style.
    if let Some(s) = strings.iter().find(|s| s.contains('@')) {
        return Some(s.clone());
    }
    // `[FooController::class, 'bar']` style.
    if let Some(pos) = stmt.find("::class") {
        let before = &stmt[..pos];
        let controller = before
            .rsplit(|c: char| c == '[' || c == '(' || c == ',' || c.is_whitespace())
            .find(|s| !s.is_empty())
            .unwrap_or("")
            .to_string();
        // The method is the string INSIDE the array (between `::class` and the
        // closing `]`) — not a later `->name('…')` route name.
        let after = &stmt[pos + "::class".len()..];
        let arr = &after[..after.find(']').unwrap_or(after.len())];
        let method = string_literals(arr).into_iter().next().unwrap_or_default();
        if !controller.is_empty() {
            return Some(if method.is_empty() {
                format!("{}::class", controller)
            } else {
                format!("{}@{}", controller, method)
            });
        }
    }
    None
}

fn normalize_method(verb: &str, stmt: &str) -> String {
    match verb {
        "match" => {
            // Route::match(['get','post'], ...) -> "GET|POST"
            let verbs = string_literals(stmt);
            let methods: Vec<String> = verbs
                .iter()
                .take_while(|s| is_http_verb(s))
                .map(|s| s.to_uppercase())
                .collect();
            if methods.is_empty() {
                "MATCH".to_string()
            } else {
                methods.join("|")
            }
        }
        "any" => "ANY".to_string(),
        "resource" => "RESOURCE".to_string(),
        "apiResource" => "API-RESOURCE".to_string(),
        "redirect" => "REDIRECT".to_string(),
        "view" => "VIEW".to_string(),
        "fallback" => "FALLBACK".to_string(),
        other => other.to_uppercase(),
    }
}

fn is_http_verb(s: &str) -> bool {
    matches!(
        s.to_lowercase().as_str(),
        "get" | "post" | "put" | "patch" | "delete" | "options" | "head"
    )
}

// ===========================================================================
// Eloquent models (docs/06 §Eloquent)
// ===========================================================================

const MODEL_BASES: &[&str] = &["Model", "Authenticatable", "Pivot", "MorphPivot"];
const RELATION_METHODS: &[&str] = &[
    "hasOne", "hasMany", "belongsTo", "belongsToMany", "hasManyThrough",
    "hasOneThrough", "morphTo", "morphOne", "morphMany", "morphToMany",
];

/// Extract Eloquent models (with table, fillable, and relationships) from a PHP file.
pub fn extract_models(rel: &str, source: &str, namespace: Option<&str>) -> Vec<crate::types::ModelInfo> {
    let mut out = Vec::new();
    for (name, base, idx) in find_classes(source) {
        let is_model = MODEL_BASES.contains(&base.as_str())
            || base.ends_with("Model")
            || rel.contains("app/Models/")
            || rel.contains("App/Models/");
        if !is_model {
            continue;
        }
        let line = line_at(source, idx);
        // Region: from this class to the next class (or end).
        let region_end = find_classes(source)
            .into_iter()
            .map(|(_, _, i)| i)
            .find(|&i| i > idx)
            .unwrap_or(source.len());
        let region = &source[idx..region_end];

        let table = property_string(region, "table");
        let fillable = property_array_strings(region, "fillable");
        let relations = extract_relations(region, idx, source);

        out.push(crate::types::ModelInfo {
            name: name.clone(),
            fqn: namespace.map(|ns| format!("{}\\{}", ns.trim_end_matches('\\'), name)),
            table,
            file: rel.to_string(),
            line,
            fillable,
            relations,
        });
    }
    out
}

/// Find `class X extends Y` declarations: (name, base, byte offset of `class`).
fn find_classes(source: &str) -> Vec<(String, String, usize)> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(p) = source[from..].find("class ") {
        let at = from + p;
        // Must be a word boundary before `class`.
        let ok = at == 0
            || !source[..at]
                .chars()
                .last()
                .map(|c| c.is_alphanumeric() || c == '_')
                .unwrap_or(false);
        from = at + 6;
        if !ok {
            continue;
        }
        let rest = &source[at + 6..];
        let name = read_ident(rest.trim_start());
        if name.is_empty() {
            continue;
        }
        // Look for `extends <Base>` on the same declaration (before `{`).
        let header_end = rest.find('{').unwrap_or(rest.len().min(300));
        let header = &rest[..header_end];
        let base = header
            .find("extends")
            .map(|e| read_ident(header[e + 7..].trim_start()))
            .map(|b| b.rsplit('\\').next().unwrap_or(&b).to_string())
            .unwrap_or_default();
        out.push((name, base, at));
    }
    out
}

/// `protected $name = 'value';` → value.
fn property_string(region: &str, prop: &str) -> Option<String> {
    let needle = format!("${}", prop);
    let idx = region.find(&needle)?;
    let after = &region[idx + needle.len()..];
    let eq = after.find('=')?;
    let semi = after[eq..].find(';').map(|s| eq + s).unwrap_or(after.len());
    string_literals(&after[eq..semi]).into_iter().next()
}

/// `protected $name = ['a', 'b'];` → ["a","b"].
fn property_array_strings(region: &str, prop: &str) -> Vec<String> {
    let needle = format!("${}", prop);
    if let Some(idx) = region.find(&needle) {
        let after = &region[idx + needle.len()..];
        if let Some(open) = after.find('[') {
            let close = after[open..].find(']').map(|c| open + c).unwrap_or(after.len());
            return string_literals(&after[open..close]);
        }
    }
    Vec::new()
}

/// Find relation methods: `function rel() { return $this->hasMany(Foo::class); }`.
fn extract_relations(region: &str, region_offset: usize, full_source: &str) -> Vec<crate::types::RelationInfo> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(p) = region[from..].find("function ") {
        let at = from + p;
        from = at + 9;
        let method = read_ident(region[at + 9..].trim_start());
        if method.is_empty() {
            continue;
        }
        // Method window: until the next `function ` or end of region.
        let win_end = region[from..]
            .find("function ")
            .map(|n| from + n)
            .unwrap_or(region.len());
        let window = &region[at..win_end];
        for rt in RELATION_METHODS {
            let call = format!("->{}(", rt);
            if let Some(cp) = window.find(&call) {
                let args = &window[cp + call.len()..];
                // Related model = identifier before `::class`, else first string.
                let related = if let Some(cc) = args.find("::class") {
                    let before = &args[..cc];
                    Some(
                        before
                            .rsplit(|c: char| c == '(' || c == ',' || c.is_whitespace())
                            .find(|s| !s.is_empty())
                            .unwrap_or("")
                            .rsplit('\\')
                            .next()
                            .unwrap_or("")
                            .to_string(),
                    )
                    .filter(|s: &String| !s.is_empty())
                } else {
                    string_literals(args).into_iter().next()
                };
                let line = line_at(full_source, region_offset + at);
                out.push(crate::types::RelationInfo {
                    method: method.clone(),
                    rel_type: rt.to_string(),
                    related,
                    line,
                });
                break;
            }
        }
    }
    out
}

// ===========================================================================
// Migration columns (real DB columns for Eloquent completion)
// ===========================================================================

const COLUMN_METHODS: &[&str] = &[
    "string", "char", "text", "mediumText", "longText", "tinyText", "integer",
    "tinyInteger", "smallInteger", "mediumInteger", "bigInteger", "unsignedBigInteger",
    "unsignedInteger", "unsignedTinyInteger", "unsignedSmallInteger", "float", "double",
    "decimal", "unsignedDecimal", "boolean", "date", "dateTime", "dateTimeTz", "time",
    "timestamp", "timestampTz", "year", "json", "jsonb", "uuid", "ulid", "ipAddress",
    "macAddress", "binary", "enum", "set", "foreignId", "foreignUuid", "foreignUlid",
    "geometry", "point",
];

fn php_type_for(method: &str) -> &'static str {
    match method {
        "boolean" => "bool",
        "integer" | "tinyInteger" | "smallInteger" | "mediumInteger" | "bigInteger"
        | "unsignedBigInteger" | "unsignedInteger" | "unsignedTinyInteger"
        | "unsignedSmallInteger" | "increments" | "bigIncrements" | "foreignId" | "year" => "int",
        "float" | "double" | "decimal" | "unsignedDecimal" => "float",
        "json" | "jsonb" => "array",
        "date" | "dateTime" | "dateTimeTz" | "timestamp" | "timestampTz" | "time" => {
            "\\Illuminate\\Support\\Carbon"
        }
        _ => "string",
    }
}

/// Extract `(table, column, php_type)` triples from a migration file.
pub fn extract_migration_columns(source: &str) -> Vec<(String, String, String)> {
    // Collect (offset, table) for each Schema::create/table block.
    let mut blocks: Vec<(usize, String)> = Vec::new();
    for needle in ["Schema::create(", "Schema::table("] {
        let mut from = 0;
        while let Some(p) = source[from..].find(needle) {
            let at = from + p;
            from = at + needle.len();
            let open = at + needle.len() - 1;
            let args = balanced_parens(source, open);
            if let Some(tbl) = string_literals(&args).into_iter().next() {
                blocks.push((at, tbl));
            }
        }
    }
    blocks.sort_by_key(|(p, _)| *p);

    let mut out: Vec<(String, String, String)> = Vec::new();
    let mut push = |tbl: &str, col: &str, ty: &str| {
        if !col.is_empty() && !out.iter().any(|(t, c, _)| t == tbl && c == col) {
            out.push((tbl.to_string(), col.to_string(), ty.to_string()));
        }
    };

    let carbon = "\\Illuminate\\Support\\Carbon";
    for i in 0..blocks.len() {
        let (start, tbl) = (blocks[i].0, blocks[i].1.clone());
        let end = blocks.get(i + 1).map(|(s, _)| *s).unwrap_or(source.len());
        let region = &source[start..end];
        let mut f = 0;
        while let Some(q) = region[f..].find("$table->") {
            let at = f + q;
            f = at + 8;
            let method: String = region[at + 8..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let rest = &region[at + 8 + method.len()..];
            let col = rest
                .find('(')
                .and_then(|op| string_literals(&rest[op..]).into_iter().next());
            match method.as_str() {
                "id" | "increments" | "bigIncrements" => push(&tbl, "id", "int"),
                "timestamps" | "timestampsTz" | "nullableTimestamps" => {
                    push(&tbl, "created_at", carbon);
                    push(&tbl, "updated_at", carbon);
                }
                "softDeletes" | "softDeletesTz" => push(&tbl, "deleted_at", carbon),
                "rememberToken" => push(&tbl, "remember_token", "string"),
                m if COLUMN_METHODS.contains(&m) => {
                    if let Some(c) = col {
                        push(&tbl, &c, php_type_for(m));
                    }
                }
                _ => {}
            }
        }
    }
    out
}

// ===========================================================================
// Config & translation keys (docs/06 §Config, §Localization)
// ===========================================================================

/// Extract dotted config keys from a `config/<name>.php` file.
/// `prefix` is the file's basename without extension (e.g. `services`).
pub fn extract_config_keys(rel: &str, source: &str, prefix: &str) -> Vec<crate::types::KeyEntry> {
    array_key_paths(source, prefix)
        .into_iter()
        .map(|(key, line)| crate::types::KeyEntry {
            key,
            locale: String::new(),
            file: rel.to_string(),
            line,
        })
        .collect()
}

/// Extract translation keys. For PHP files prefix = basename (e.g. `auth`),
/// locale = the lang sub-directory (e.g. `en`). JSON files are flat.
pub fn extract_translations(
    rel: &str,
    source: &str,
    prefix: &str,
    locale: &str,
    is_json: bool,
) -> Vec<crate::types::KeyEntry> {
    if is_json {
        // Flat key→string JSON map.
        let mut out = Vec::new();
        if let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(source) {
            for (k, _) in map {
                out.push(crate::types::KeyEntry {
                    key: k,
                    locale: locale.to_string(),
                    file: rel.to_string(),
                    line: 1,
                });
            }
        }
        return out;
    }
    array_key_paths(source, prefix)
        .into_iter()
        .map(|(key, line)| crate::types::KeyEntry {
            key,
            locale: locale.to_string(),
            file: rel.to_string(),
            line,
        })
        .collect()
}

/// Walk a PHP `return [...]` array and produce dotted key paths
/// (e.g. `services.stripe.key`), each prefixed with `prefix`.
fn array_key_paths(source: &str, prefix: &str) -> Vec<(String, u32)> {
    let mut out = Vec::new();
    let chars: Vec<char> = source.chars().collect();
    let mut byte_pos = 0usize; // byte index for line calc
    let mut stack: Vec<String> = Vec::new();
    let mut last_string: Option<(String, usize)> = None;
    let mut pending_key: Option<String> = None;

    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        // string literal
        if c == '\'' || c == '"' {
            let quote = c;
            let str_start = byte_pos;
            let mut lit = String::new();
            i += 1;
            byte_pos += c.len_utf8();
            let mut prev = '\0';
            while i < chars.len() {
                let ch = chars[i];
                byte_pos += ch.len_utf8();
                i += 1;
                if ch == quote && prev != '\\' {
                    break;
                }
                lit.push(ch);
                prev = ch;
            }
            last_string = Some((lit, str_start));
            continue;
        }
        // `=>`
        if c == '=' && i + 1 < chars.len() && chars[i + 1] == '>' {
            if let Some((key, start)) = last_string.clone() {
                pending_key = Some(key.clone());
                let mut parts = vec![prefix.to_string()];
                parts.extend(stack.iter().cloned());
                parts.push(key);
                let path: Vec<String> = parts.into_iter().filter(|s| !s.is_empty()).collect();
                out.push((path.join("."), line_at(source, start)));
            }
            i += 2;
            byte_pos += 2;
            continue;
        }
        if c == '[' {
            stack.push(pending_key.take().unwrap_or_default());
        } else if c == ']' {
            stack.pop();
        }
        byte_pos += c.len_utf8();
        i += 1;
    }
    out
}

/// A Laravel "string key" call usage: (kind, key, byte offset of the key).
/// kind ∈ {route, config, trans, view}.
pub fn key_usages(source: &str) -> Vec<(String, String, usize)> {
    let calls: &[(&str, &str)] = &[
        ("route(", "route"),
        ("config(", "config"),
        ("__(", "trans"),
        ("trans(", "trans"),
        ("trans_choice(", "trans"),
        ("@lang(", "trans"),
        ("view(", "view"),
    ];
    let mut out = Vec::new();
    for (needle, kind) in calls {
        let mut from = 0;
        while let Some(p) = source[from..].find(needle) {
            let open = from + p + needle.len() - 1; // index of '('
            from = open + 1;
            // first string literal right after '('
            let frag = &source[open..];
            if let Some(q) = frag.find(['\'', '"']) {
                let quote = frag.as_bytes()[q] as char;
                let key_start = open + q + 1;
                if let Some(end) = source[key_start..].find(quote) {
                    let key = source[key_start..key_start + end].to_string();
                    // Ignore dynamic / placeholder keys: `$var`, `{id}`, `<page>`,
                    // wildcards, interpolation, or anything not a plain dotted key.
                    let dynamic = key.is_empty()
                        || key
                            .chars()
                            .any(|c| !(c.is_alphanumeric() || c == '.' || c == '_' || c == '-'));
                    if !dynamic {
                        out.push((kind.to_string(), key, key_start));
                    }
                }
            }
        }
    }
    out
}

/// Middleware aliases declared in Kernel.php / bootstrap/app.php:
/// captures the string key of any `'alias' => Something::class` pair.
pub fn parse_middleware_aliases(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(p) = source[from..].find("=>") {
        let at = from + p;
        from = at + 2;
        // value: up to next comma/newline must mention ::class
        let after = &source[at + 2..];
        let valend = after.find([',', '\n']).unwrap_or(after.len());
        if !after[..valend].contains("::class") {
            continue;
        }
        // key: the last string literal before `=>`
        let before = &source[..at];
        if let Some(qpos) = before.rfind(['\'', '"']) {
            let quote = before.as_bytes()[qpos] as char;
            if let Some(open) = before[..qpos].rfind(quote) {
                let key = &before[open + 1..qpos];
                if !key.is_empty()
                    && key.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '_' || c == '-')
                {
                    let k = key.to_string();
                    if !out.contains(&k) {
                        out.push(k);
                    }
                }
            }
        }
    }
    out
}

/// Field keys from a FormRequest's `rules()` return array (or `$rules`).
pub fn rules_keys(source: &str) -> Vec<String> {
    let region_start = source
        .find("function rules")
        .or_else(|| source.find("$rules"));
    if let Some(p) = region_start {
        if let Some(op) = source[p..].find('[') {
            let region = balanced_brackets(source, p + op);
            return array_key_paths(&region, "")
                .into_iter()
                .map(|(k, _)| k)
                .filter(|k| !k.is_empty())
                .collect();
        }
    }
    Vec::new()
}

fn line_at(source: &str, byte_idx: usize) -> u32 {
    source.as_bytes()[..byte_idx.min(source.len())]
        .iter()
        .filter(|&&b| b == b'\n')
        .count() as u32
        + 1
}

// ===========================================================================
// Service container bindings (docs/06 §Container)
// ===========================================================================

const BIND_METHODS: &[&str] = &["bind", "singleton", "instance", "scoped", "alias"];

/// Capture the substring inside a `(...)` starting at `open` (index of '(').
fn balanced_parens(source: &str, open: usize) -> String {
    let mut depth = 0i32;
    let mut buf = String::new();
    for (i, ch) in source[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            _ => {}
        }
        if i > 0 {
            buf.push(ch);
        }
        if buf.len() > 2000 {
            break;
        }
    }
    buf
}

/// First `X::class` identifier in a fragment, reduced to its short name.
fn first_class_const(frag: &str) -> Option<String> {
    let pos = frag.find("::class")?;
    let before = &frag[..pos];
    let ident = before
        .rsplit(|c: char| !(c.is_alphanumeric() || c == '_' || c == '\\'))
        .find(|s| !s.is_empty())?;
    Some(ident.rsplit('\\').next().unwrap_or(ident).to_string())
}

pub fn extract_bindings(rel: &str, source: &str) -> Vec<crate::types::Binding> {
    let mut out = Vec::new();
    for m in BIND_METHODS {
        let needle = format!("->{}(", m);
        let mut from = 0;
        while let Some(p) = source[from..].find(&needle) {
            let call = from + p;
            let open = call + needle.len() - 1; // index of '('
            from = open + 1;
            let args = balanced_parens(source, open);
            let strings = string_literals(&args);
            // abstract = first ::class or first string
            let abstract_name = first_class_const(&args)
                .or_else(|| strings.first().cloned())
                .unwrap_or_default();
            if abstract_name.is_empty() {
                continue;
            }
            // concrete = a second ::class (after the first) or a closure
            let concrete = {
                if args.contains("function") || args.contains("fn ") || args.contains("fn(") {
                    Some("Closure".to_string())
                } else {
                    // look for a class const after the first comma
                    args.find(',')
                        .and_then(|c| first_class_const(&args[c..]))
                        .or_else(|| strings.get(1).cloned())
                }
            };
            out.push(crate::types::Binding {
                abstract_name,
                concrete,
                kind: m.to_string(),
                file: rel.to_string(),
                line: line_at(source, call),
            });
        }
    }
    out
}

// ===========================================================================
// Events & listeners (docs/06 §Events)
// ===========================================================================

/// Parse `protected $listen = [ Event::class => [ Listener::class, ... ] ]`.
pub fn extract_events(rel: &str, source: &str) -> Vec<crate::types::EventListener> {
    let mut out = Vec::new();
    let start = match source.find("$listen") {
        Some(s) => s,
        None => return out,
    };
    let open = match source[start..].find('[') {
        Some(o) => start + o,
        None => return out,
    };
    let region = balanced_brackets(source, open);
    // Walk class consts; a class followed by `=>` is an event, others are its listeners.
    let mut current_event: Option<String> = None;
    let mut idx = 0usize;
    while let Some(p) = region[idx..].find("::class") {
        let at = idx + p;
        let before = &region[..at];
        let ident = before
            .rsplit(|c: char| !(c.is_alphanumeric() || c == '_' || c == '\\'))
            .find(|s| !s.is_empty())
            .unwrap_or("")
            .rsplit('\\')
            .next()
            .unwrap_or("")
            .to_string();
        idx = at + 7;
        let after = region[idx..].trim_start();
        if after.starts_with("=>") {
            current_event = Some(ident);
        } else if let Some(ev) = &current_event {
            if !ident.is_empty() {
                out.push(crate::types::EventListener {
                    event: ev.clone(),
                    listener: ident,
                    file: rel.to_string(),
                    line: line_at(source, open + at),
                });
            }
        }
    }
    out
}

fn balanced_brackets(source: &str, open: usize) -> String {
    let mut depth = 0i32;
    let mut buf = String::new();
    for ch in source[open..].chars() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                buf.push(ch);
                if depth == 0 {
                    break;
                }
                continue;
            }
            _ => {}
        }
        buf.push(ch);
        if buf.len() > 20000 {
            break;
        }
    }
    buf
}

// ===========================================================================
// Queued jobs + factories/seeders (docs/06 §Queues, §Eloquent)
// ===========================================================================

pub fn extract_jobs(rel: &str, source: &str) -> Vec<crate::types::JobInfo> {
    let mut out = Vec::new();
    let is_job_path = rel.contains("app/Jobs/") || rel.contains("App/Jobs/");
    let queued = source.contains("ShouldQueue");
    if !is_job_path && !queued {
        return out;
    }
    for (name, _base, idx) in find_classes(source) {
        out.push(crate::types::JobInfo {
            name,
            fqn: None,
            queued,
            file: rel.to_string(),
            line: line_at(source, idx),
        });
    }
    out
}

pub fn extract_artifacts(rel: &str, source: &str) -> Vec<crate::types::ArtifactInfo> {
    let mut out = Vec::new();
    for (name, base, idx) in find_classes(source) {
        let kind = if base == "Factory" || rel.contains("database/factories") {
            "factory"
        } else if base == "Seeder" || rel.contains("database/seeders") {
            "seeder"
        } else {
            continue;
        };
        let related = if kind == "factory" {
            // protected $model = User::class;
            source
                .find("$model")
                .and_then(|m| first_class_const(&source[m..m + 120.min(source.len() - m)]))
        } else {
            None
        };
        out.push(crate::types::ArtifactInfo {
            name,
            kind: kind.to_string(),
            related,
            file: rel.to_string(),
            line: line_at(source, idx),
        });
    }
    out
}
