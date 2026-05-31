//! Search Everywhere ranking (MVP slice of `docs/02-module-design.md` §navigation).
//!
//! Pulls candidates from the index across categories (files, symbols, routes)
//! and ranks them with a CamelHumps-aware fuzzy scorer, returning the top-K.

use crate::db::Index;
use crate::types::SearchHit;

/// Run a unified search across files, symbols, and routes.
pub fn search_everywhere(index: &Index, query: &str, limit: usize) -> Vec<SearchHit> {
    let q = query.trim();
    if q.is_empty() {
        return Vec::new();
    }
    let mut hits: Vec<SearchHit> = Vec::new();

    // Symbols
    if let Ok(cands) = index.symbol_candidates(q, 400) {
        for s in cands {
            if let Some(score) = fuzzy_score(q, &s.name) {
                // Keep first-party symbols above framework/package symbols.
                let vendor_penalty = if s.file.contains("/vendor/") { -60 } else { 0 };
                hits.push(SearchHit {
                    category: "symbol".into(),
                    label: s.name.clone(),
                    detail: s
                        .fqn
                        .clone()
                        .or_else(|| s.container.clone())
                        .unwrap_or_else(|| s.file.clone()),
                    file: s.file,
                    line: s.line,
                    score: score + kind_weight(s.kind.as_str()) + vendor_penalty,
                    kind: Some(s.kind.as_str().to_string()),
                });
            }
        }
    }

    // Files (match against the basename primarily)
    if let Ok(cands) = index.file_candidates(q, 400) {
        for f in cands {
            let base = f.path.rsplit('/').next().unwrap_or(&f.path);
            if let Some(score) = fuzzy_score(q, base) {
                hits.push(SearchHit {
                    category: "file".into(),
                    label: base.to_string(),
                    detail: f.path.clone(),
                    file: f.path,
                    line: 1,
                    score: score + if f.is_vendor { -50 } else { 0 },
                    kind: None,
                });
            }
        }
    }

    // Routes
    if let Ok(cands) = index.route_candidates(q, 200) {
        for r in cands {
            let hay = format!("{} {}", r.uri, r.name.clone().unwrap_or_default());
            if let Some(score) = fuzzy_score(q, &hay) {
                hits.push(SearchHit {
                    category: "route".into(),
                    label: format!("{} {}", r.method, r.uri),
                    detail: r
                        .name
                        .clone()
                        .map(|n| format!("name: {}", n))
                        .or_else(|| r.action.clone())
                        .unwrap_or_default(),
                    file: r.file,
                    line: r.line,
                    score: score + 10,
                    kind: None,
                });
            }
        }
    }

    hits.sort_by(|a, b| b.score.cmp(&a.score).then(a.label.len().cmp(&b.label.len())));
    hits.truncate(limit);
    hits
}

fn kind_weight(kind: &str) -> i64 {
    match kind {
        "class" | "interface" | "trait" | "enum" => 30,
        "function" | "method" => 20,
        "constant" | "enum_case" => 10,
        _ => 0,
    }
}

/// A small subsequence fuzzy matcher with bonuses for:
/// - exact / prefix matches
/// - matches at word boundaries / CamelHumps (uppercase, after `_`/`\`/`/`)
/// - contiguous runs
/// Returns `None` if `needle` is not a subsequence of `haystack`.
pub fn fuzzy_score(needle: &str, haystack: &str) -> Option<i64> {
    let n: Vec<char> = needle.to_lowercase().chars().collect();
    let h: Vec<char> = haystack.chars().collect();
    let hl: Vec<char> = haystack.to_lowercase().chars().collect();
    if n.is_empty() {
        return Some(0);
    }

    let mut score: i64 = 0;
    let mut ni = 0usize;
    let mut prev_match: Option<usize> = None;

    for (hi, &hc) in hl.iter().enumerate() {
        if ni >= n.len() {
            break;
        }
        if hc == n[ni] {
            score += 1;
            // word-boundary / CamelHump bonus
            let is_boundary = hi == 0
                || matches!(h.get(hi.wrapping_sub(1)), Some('_') | Some('\\') | Some('/') | Some('.'))
                || h.get(hi).map(|c| c.is_uppercase()).unwrap_or(false);
            if is_boundary {
                score += 8;
            }
            // contiguity bonus
            if let Some(p) = prev_match {
                if hi == p + 1 {
                    score += 5;
                }
            }
            prev_match = Some(hi);
            ni += 1;
        }
    }

    if ni != n.len() {
        return None; // not all needle chars matched in order
    }

    // exact & prefix bonuses
    let hay_lower: String = hl.iter().collect();
    let needle_lower: String = n.iter().collect();
    if hay_lower == needle_lower {
        score += 100;
    } else if hay_lower.starts_with(&needle_lower) {
        score += 40;
    }
    // shorter haystacks rank a touch higher for equal matches
    score -= (h.len() as i64) / 20;
    Some(score)
}
