//! AST-aware structural selection — the engine behind Ctrl+W / Ctrl+Shift+W.
//!
//! Given the current Monaco selection (1-based line/col), walks the tree-sitter
//! CST upward until it finds a node that strictly contains the selection, then
//! returns that node's range. The frontend keeps a selection stack so Ctrl+W
//! can be repeatedly expanded and Ctrl+Shift+W can shrink back.

use serde::{Deserialize, Serialize};
use tree_sitter::{Node, Parser};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectionRange {
    /// All values are 1-based (Monaco convention).
    pub start_line: u32,
    pub start_col:  u32,
    pub end_line:   u32,
    pub end_col:    u32,
}

/// Expand the current selection to the next larger AST node.
///
/// Returns `None` when the selection already spans the entire file root or the
/// source cannot be parsed.
pub fn expand_selection(
    source: &str,
    start_line: u32,
    start_col:  u32,
    end_line:   u32,
    end_col:    u32,
) -> Option<SelectionRange> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_php::LANGUAGE_PHP.into())
        .ok()?;
    let tree  = parser.parse(source, None)?;
    let bytes = source.as_bytes();

    // Convert 1-based Monaco positions → 0-based byte offsets.
    let sel_start = line_col_to_byte(bytes, (start_line - 1) as usize, (start_col - 1) as usize);
    let sel_end   = line_col_to_byte(bytes, (end_line - 1) as usize,   (end_col - 1) as usize);

    let root     = tree.root_node();
    let innermost = innermost_containing(root, sel_start, sel_end)?;

    // If the innermost node exactly matches the current selection, move up to
    // the parent so we always return something *larger* than what the user has.
    let target = if innermost.start_byte() == sel_start && innermost.end_byte() == sel_end {
        innermost.parent().unwrap_or(innermost)
    } else {
        innermost
    };

    let p = target.start_position();
    let q = target.end_position();
    Some(SelectionRange {
        start_line: p.row as u32 + 1,
        start_col:  p.column as u32 + 1,
        end_line:   q.row as u32 + 1,
        end_col:    q.column as u32 + 1,
    })
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Convert a (0-based row, 0-based column) pair to a byte offset in `source`.
fn line_col_to_byte(source: &[u8], row: usize, col: usize) -> usize {
    let mut cur_row = 0usize;
    let mut row_start = 0usize;
    for (i, &b) in source.iter().enumerate() {
        if cur_row == row {
            return row_start + col.min(source.len() - row_start);
        }
        if b == b'\n' {
            cur_row += 1;
            row_start = i + 1;
        }
    }
    source.len()
}

/// Return the deepest (innermost) tree-sitter node whose byte range fully
/// contains [start, end]. Prefers named nodes over anonymous punctuation.
fn innermost_containing<'a>(node: Node<'a>, start: usize, end: usize) -> Option<Node<'a>> {
    // Prune: if this node doesn't cover the selection at all, bail.
    if node.start_byte() > start || node.end_byte() < end {
        return None;
    }

    // Try to descend into a child that still covers the whole selection.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(n) = innermost_containing(child, start, end) {
            return Some(n);
        }
    }

    // No child fully covers it → this node is the innermost containing node.
    Some(node)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "<?php\nclass Foo {\n    public function bar(): void {\n        $x = 1 + 2;\n    }\n}\n";

    #[test]
    fn expands_cursor_to_token() {
        // Line 4 col 14 is "1" in "$x = 1 + 2;"
        let r = expand_selection(SRC, 4, 14, 4, 14).expect("some result");
        assert!(r.start_line <= 4 && r.end_line >= 4, "should be on line 4");
        // start_col < 14 or end_col > 14 → expanded beyond cursor
        assert!(r.start_col < 14 || r.end_col > 14, "should expand beyond cursor");
    }

    #[test]
    fn expands_word_to_expression() {
        // Select just "1" (col 14..15 on line 4)
        let r1 = expand_selection(SRC, 4, 14, 4, 15).expect("r1");
        // Expanding "1" should give us a bigger expression or statement.
        let r2 = expand_selection(SRC, r1.start_line, r1.start_col, r1.end_line, r1.end_col)
            .expect("r2");
        assert!(
            (r2.end_col - r2.start_col) > (r1.end_col - r1.start_col)
                || r2.end_line > r1.end_line
                || r2.start_line < r1.start_line,
            "second expand should be larger"
        );
    }
}
