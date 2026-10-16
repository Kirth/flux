#[derive(Debug, Clone, PartialEq)]
pub enum SizeOp {
    Gt(u64),
    Lt(u64),
    Range(u64, u64),
}

/// Parse size filter from query.
/// Returns (SizeOp, remaining_query) or None.
pub fn parse_size_filter(query: &str) -> Option<(SizeOp, String)> {
    let query_lc = query.to_lowercase();
    let parts: Vec<&str> = query_lc.split_whitespace().collect();

    for (i, part) in parts.iter().enumerate() {
        if let Some(op) = parse_size_op(part) {
            let rest = parts
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, s)| *s)
                .collect::<Vec<_>>()
                .join(" ");
            return Some((op, rest));
        }
    }
    None
}

/// One size token: `>50mb`, `<1g`, `10mb..1gb`.
pub(super) fn parse_size_op(s: &str) -> Option<SizeOp> {
    if let Some((left, right)) = s.split_once("..") {
        let l = parse_size(left)?;
        let r = parse_size(right)?;
        return Some(SizeOp::Range(l, r));
    }

    if let Some(rest) = s.strip_prefix('>') {
        return Some(SizeOp::Gt(parse_size(rest)?));
    }
    if let Some(rest) = s.strip_prefix('<') {
        return Some(SizeOp::Lt(parse_size(rest)?));
    }

    None
}

fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    let num_end = s
        .find(|c: char| !c.is_numeric() && c != '.')
        .unwrap_or(s.len());

    if num_end == 0 {
        return None;
    }

    let (num_part, unit_part) = s.split_at(num_end);
    // Whole or decimal (`1.5gb`); rounded to bytes below.
    let n: f64 = num_part.parse().ok()?;
    if !n.is_finite() || n < 0.0 {
        return None;
    }

    let unit_clean = unit_part.trim().to_lowercase();
    let unit = unit_clean
        .strip_suffix("ib")
        .or_else(|| unit_clean.strip_suffix('b'))
        .unwrap_or(&unit_clean);

    // A size filter must specify a real unit. Bare numbers (`>10`) and
    // explicit byte units (`>100B`) are rejected because they match
    // essentially everything and can lock up the UI on large trees.
    if unit.is_empty() {
        return None;
    }

    let multiplier: u64 = match unit {
        "k" => 1024,
        "m" => 1024 * 1024,
        "g" => 1024 * 1024 * 1024,
        "t" => 1024 * 1024 * 1024 * 1024,
        _ => return None,
    };

    let bytes = (n * multiplier as f64).round();
    (bytes < u64::MAX as f64).then_some(bytes as u64)
}
