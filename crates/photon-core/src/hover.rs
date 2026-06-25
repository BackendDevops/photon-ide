//! Position-aware hover context extractor.
//!
//! Given (line, col) in a PHP source buffer, resolves the token under the
//! cursor and its receiver so the Tauri command layer can pick the right
//! symbol from the index — the key piece that makes `$user->getName()` hover
//! resolve to `User::getName` rather than any random `getName` in the project.

/// Whether the token is accessed via `->` (dynamic) or `::` (static/class).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessKind {
    Dynamic, // `->`
    Static,  // `::`
}

/// The word under the cursor plus everything the resolver needs to look it up.
#[derive(Debug, Clone)]
pub struct TokenContext {
    /// The identifier under the cursor (no `$` prefix).
    pub word: String,
    /// Raw receiver token as it appears in source (e.g. `"$user"`, `"self"`,
    /// `"UserService"`). `None` when the word is standalone (not a member).
    pub receiver: Option<String>,
    pub access: Option<AccessKind>,
    /// 1-based column range of the word on its line.
    pub col_start: u32,
    pub col_end: u32,
}

/// Extract the PHP identifier at 1-based `(line, col)` together with its
/// receiver context. Returns `None` when the cursor sits on whitespace,
/// punctuation, or a PHP keyword with no navigation value.
pub fn token_context_at(source: &str, line: u32, col: u32) -> Option<TokenContext> {
    let src_line = source.lines().nth(line.saturating_sub(1) as usize)?;
    let (word, col_start, col_end) = word_at(src_line, col)?;

    // Skip keywords and very short tokens that can't be meaningful symbols.
    let bare = word.trim_start_matches('$');
    if bare.len() < 2 || is_keyword(bare) {
        return None;
    }

    // Walk left of the word to detect `->` / `::` and the receiver token.
    let (receiver, access) = receiver_before(src_line, col_start);

    Some(TokenContext {
        word: bare.to_string(),
        receiver,
        access,
        col_start,
        col_end,
    })
}

/// Extract the word that covers column `col` (1-based) on `line`.
/// Returns `(word, col_start, col_end)` with 1-based columns.
fn word_at(line: &str, col: u32) -> Option<(String, u32, u32)> {
    let bytes = line.as_bytes();
    let col0 = col.saturating_sub(1) as usize;
    if col0 >= bytes.len() {
        return None;
    }

    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b == b'\\';

    // If cursor is on a non-ident character, try one position left (common with
    // end-of-word clicks).
    let pivot = if is_ident(bytes[col0]) {
        col0
    } else if col0 > 0 && is_ident(bytes[col0 - 1]) {
        col0 - 1
    } else {
        return None;
    };

    let mut start = pivot;
    while start > 0 && is_ident(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = pivot + 1;
    while end < bytes.len() && is_ident(bytes[end]) {
        end += 1;
    }

    let word = line[start..end].to_string();
    if word.is_empty() {
        return None;
    }
    Some((word, start as u32 + 1, end as u32 + 1))
}

/// Walk left of `col_start` on `line` to find `->receiver` or `::receiver`.
/// Returns `(receiver_token, access_kind)`.
fn receiver_before(line: &str, col_start: u32) -> (Option<String>, Option<AccessKind>) {
    let prefix = &line[..col_start.saturating_sub(1) as usize];
    let trimmed = prefix.trim_end();

    let (access, after_op) = if let Some(rest) = trimmed.strip_suffix("->") {
        (AccessKind::Dynamic, rest.trim_end())
    } else if let Some(rest) = trimmed.strip_suffix("::") {
        (AccessKind::Static, rest.trim_end())
    } else {
        return (None, None);
    };

    // Extract the receiver token immediately before the operator.
    // Handles `$var`, `ClassName`, `)` (chained calls — we just return `)` and
    // the resolver skips chain resolution for now).
    let is_recv = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b == b'\\';
    let recv_bytes = after_op.as_bytes();
    let mut end = recv_bytes.len();
    while end > 0 && is_recv(recv_bytes[end - 1]) {
        end -= 1;
    }
    let receiver = &after_op[end..];

    if receiver.is_empty() {
        (None, Some(access))
    } else {
        (Some(receiver.to_string()), Some(access))
    }
}

/// PHP keywords that are never navigable symbols.
fn is_keyword(s: &str) -> bool {
    matches!(
        s,
        "if" | "else" | "elseif" | "while" | "for" | "foreach" | "do" | "switch"
            | "case" | "break" | "continue" | "return" | "function" | "class"
            | "interface" | "trait" | "enum" | "extends" | "implements" | "use"
            | "namespace" | "new" | "echo" | "print" | "var" | "const" | "static"
            | "public" | "private" | "protected" | "abstract" | "final" | "readonly"
            | "true" | "false" | "null" | "match" | "throw" | "try" | "catch"
            | "finally" | "yield" | "fn" | "array" | "list" | "void" | "int"
            | "float" | "string" | "bool" | "mixed" | "never" | "object"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_plain_word() {
        let src = "<?php\n$user = new User();\n$user->getName();\n";
        let ctx = token_context_at(src, 3, 8).unwrap();
        assert_eq!(ctx.word, "getName");
        assert_eq!(ctx.access, Some(AccessKind::Dynamic));
        assert_eq!(ctx.receiver.as_deref(), Some("$user"));
    }

    #[test]
    fn extracts_static_call() {
        let src = "<?php\nUser::find(1);\n";
        let ctx = token_context_at(src, 2, 7).unwrap();
        assert_eq!(ctx.word, "find");
        assert_eq!(ctx.access, Some(AccessKind::Static));
        assert_eq!(ctx.receiver.as_deref(), Some("User"));
    }

    #[test]
    fn extracts_standalone_class() {
        let src = "<?php\n$u = new UserController();\n";
        let ctx = token_context_at(src, 2, 13).unwrap();
        assert_eq!(ctx.word, "UserController");
        assert_eq!(ctx.access, None);
        assert_eq!(ctx.receiver, None);
    }

    #[test]
    fn skips_keywords() {
        let src = "<?php\nforeach ($items as $item) {}\n";
        assert!(token_context_at(src, 2, 2).is_none()); // "foreach"
    }
}
