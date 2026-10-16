//! The search box query: name terms, globs, size and date filters, parsed once and
//! matched the same way by the view filter, the folder loader and "open/select matches".

use super::fuzzy::fuzzy_match_compiled;
use super::size::{parse_size_op, SizeOp};
use globset::GlobSet;
use nucleo::pattern::{CaseMatching, Normalization, Pattern};

#[derive(Debug, Clone, Default)]
pub struct SearchQuery {
    /// Fuzzy name terms; all must match.
    pub terms: Vec<String>,
    /// Lowercase filename globs (`*.webm`); any may match. Present = search subfolders.
    pub globs: Vec<String>,
    pub size: Option<SizeOp>,
    /// Unix time; only files modified after it (the search panel's `>date:N`).
    pub modified_after: Option<i64>,
    pattern: Option<Pattern>,
    globset: Option<GlobSet>,
}

/// Size units that may follow a number as their own word (`>50 MB`).
const UNITS: [&str; 12] = [
    "k", "kb", "kib", "m", "mb", "mib", "g", "gb", "gib", "t", "tb", "tib",
];

/// Splits `query` into words, joining a lone `>`/`<` with the size after it
/// (`> 50MB`) and a number with a unit after it (`>50 MB`).
fn tokens(query: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut words = query.split_whitespace().peekable();
    while let Some(word) = words.next() {
        let mut token = word.to_lowercase();
        if matches!(token.as_str(), ">" | "<" | ">=" | "<=") {
            if let Some(next) = words.next() {
                token = format!("{}{}", token.trim_end_matches('='), next.to_lowercase());
            }
        } else if let Some(op) = token
            .strip_suffix('=')
            .filter(|_| token.starts_with('>') || token.starts_with('<'))
        {
            token = op.to_string();
        }
        let ends_in_digit = token.chars().last().is_some_and(|c| c.is_ascii_digit());
        if ends_in_digit
            && (token.starts_with('>') || token.starts_with('<') || token.contains(".."))
        {
            if let Some(unit) = words.peek().map(|w| w.to_lowercase()) {
                if UNITS.contains(&unit.as_str()) {
                    token.push_str(&unit);
                    words.next();
                }
            }
        }
        out.push(token);
    }
    out
}

/// `size:>1048576` (raw bytes, as the search panel writes it).
fn parse_raw_size(token: &str) -> Option<SizeOp> {
    let rest = token.strip_prefix("size:")?;
    if let Some(n) = rest.strip_prefix('>') {
        return n.parse().ok().map(SizeOp::Gt);
    }
    rest.strip_prefix('<')?.parse().ok().map(SizeOp::Lt)
}

pub fn parse_query(query: &str) -> SearchQuery {
    let mut q = SearchQuery::default();
    for token in tokens(query) {
        if let Some(size) = parse_raw_size(&token).or_else(|| parse_size_op(&token)) {
            q.size = Some(size);
        } else if let Some(t) = token.strip_prefix(">date:").and_then(|n| n.parse().ok()) {
            q.modified_after = Some(t);
        } else if token.contains(['*', '?', '[']) {
            q.globs.push(token);
        } else if token.len() > 1 && token.starts_with('.') && !token[1..].contains('.') {
            // `.webm`: files with that extension, in subfolders too.
            q.globs.push(format!("*{token}"));
        } else {
            q.terms.push(token);
        }
    }
    if !q.terms.is_empty() {
        q.pattern = Some(Pattern::parse(
            &q.terms.join(" "),
            CaseMatching::Ignore,
            Normalization::Smart,
        ));
    }
    q.globset = crate::utils::glob::compile_patterns(&q.globs);
    q
}

impl SearchQuery {
    /// Nothing to filter on.
    pub fn is_empty(&self) -> bool {
        self.terms.is_empty()
            && self.globs.is_empty()
            && self.size.is_none()
            && self.modified_after.is_none()
    }

    /// Globs search the whole tree below the current folder; the rest filters it.
    pub fn is_deep(&self) -> bool {
        !self.globs.is_empty()
    }

    /// Names must match the terms and globs; size and date only apply to files
    /// (folders stay visible so they can still be opened).
    pub fn matches(&self, name: &str, is_dir: bool, size: u64, mtime: i64) -> bool {
        if let Some(globset) = &self.globset {
            if !globset.is_match(name.to_lowercase()) {
                return false;
            }
        }
        if let Some(pattern) = &self.pattern {
            if !fuzzy_match_compiled(name, pattern) {
                return false;
            }
        }
        if is_dir {
            return true;
        }
        let size_ok = match self.size {
            Some(SizeOp::Gt(v)) => size > v,
            Some(SizeOp::Lt(v)) => size < v,
            Some(SizeOp::Range(l, r)) => size >= l.min(r) && size <= l.max(r),
            None => true,
        };
        size_ok && self.modified_after.is_none_or(|t| mtime > t)
    }
}
