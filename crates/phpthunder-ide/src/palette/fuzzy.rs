//! Fuzzy matching used to rank files and symbols.

/// Fuzzy subsequence match. Returns a score (higher is better) and the
/// matched character indices, or `None` when not all query chars appear.
pub fn fuzzy(query: &str, candidate: &str) -> Option<(i32, Vec<usize>)> {
    if query.is_empty() {
        return Some((0, Vec::new()));
    }
    let chars: Vec<char> = candidate.chars().collect();
    let lower: Vec<char> = chars.iter().map(|c| c.to_ascii_lowercase()).collect();
    let q: Vec<char> = query
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    let mut matched = Vec::with_capacity(q.len());
    let mut score = 0i32;
    let mut qi = 0;
    let mut last: Option<usize> = None;
    for (i, &c) in lower.iter().enumerate() {
        if qi == q.len() {
            break;
        }
        if c != q[qi] {
            continue;
        }
        let boundary = i == 0
            || matches!(chars[i - 1], '\\' | '/' | '_' | '-' | '.' | ' ' | ':' | '$')
            || (chars[i].is_uppercase() && chars[i - 1].is_lowercase());
        score += 1;
        if boundary {
            score += 12;
        }
        match last {
            Some(l) if l + 1 == i => score += 8,
            Some(l) => score -= ((i - l - 1) as i32).min(6),
            None => score -= (i as i32).min(8),
        }
        matched.push(i);
        last = Some(i);
        qi += 1;
    }
    if qi < q.len() {
        return None;
    }
    // Prefer shorter candidates and exact case.
    score -= (chars.len() as i32 / 8).min(10);
    if candidate.starts_with(query) {
        score += 20;
    }
    Some((score, matched))
}

#[cfg(test)]
mod tests {
    use super::fuzzy;

    #[test]
    fn fuzzy_prefers_boundaries_and_prefixes() {
        let (a, _) = fuzzy("uc", "UserController.php").unwrap();
        let (b, _) = fuzzy("uc", "Structure.php").unwrap();
        assert!(a > b);
        assert!(fuzzy("xyz", "User.php").is_none());
        let (_, m) = fuzzy("pc", "PostController").unwrap();
        assert_eq!(m, vec![0, 4]);
        let (exact, _) = fuzzy("Post", "Post").unwrap();
        let (longer, _) = fuzzy("Post", "PostController").unwrap();
        assert!(exact > longer);
    }
}
