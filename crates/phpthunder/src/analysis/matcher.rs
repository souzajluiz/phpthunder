//! Matching typed text against completion candidates: plain prefixes, camel
//! humps (`gUN` and `gun` find `getUserName`, `fb` finds `foo_bar`) and
//! word-start matches in the middle of a name (`Name` finds
//! `getUserName`). Each match reports its quality and the matched ranges,
//! so candidates can be ranked and the matched letters highlighted.

use std::ops::Range;

/// How well a name matches, best last (so `Ord` sorts worst first).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MatchTier {
    /// The pattern matches word starts after the first word (`Name` in
    /// `getUserName`).
    Middle,
    /// The pattern matches camel humps from the first letter (`gUN`).
    Humps,
    /// The name starts with the pattern, ignoring case.
    PrefixIgnoringCase,
    /// The name starts with the pattern, case included.
    Prefix,
    /// The name is the pattern.
    Exact,
}

/// A successful match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NameMatch {
    /// Quality of the match.
    pub tier: MatchTier,
    /// Byte ranges of the name that matched, in order, merged when adjacent.
    pub fragments: Vec<Range<usize>>,
}

/// Matches names against one typed pattern. Build once per completion
/// request with [`Matcher::new`] and call [`Matcher::matches`] per name.
#[derive(Clone, Debug)]
pub struct Matcher {
    /// The pattern as typed, without a leading `$`.
    pattern: Vec<char>,
    /// Whether the pattern started with `$` (variables).
    dollar: bool,
}

/// Whether `chars[i]` starts a word: the first letter, an uppercase letter
/// after a lowercase one or a digit (or before a lowercase one, for
/// acronyms like `HTMLParser`), a letter after `_`, `\`, `-` or `$`, or a
/// digit after a letter.
fn is_word_start(chars: &[char], i: usize) -> bool {
    if i == 0 {
        return true;
    }
    let (prev, cur) = (chars[i - 1], chars[i]);
    if matches!(prev, '_' | '\\' | '-' | '$' | '.' | ':') {
        return cur.is_alphanumeric();
    }
    if cur.is_uppercase() {
        return prev.is_lowercase()
            || prev.is_ascii_digit()
            || chars.get(i + 1).is_some_and(|n| n.is_lowercase()) && prev.is_uppercase();
    }
    cur.is_ascii_digit() && prev.is_alphabetic()
}

/// Case-insensitive character equality.
fn same_letter(a: char, b: char) -> bool {
    a == b || a.to_lowercase().eq(b.to_lowercase())
}

impl Matcher {
    /// A matcher for `pattern` (the text typed before the caret).
    pub fn new(pattern: &str) -> Self {
        let dollar = pattern.starts_with('$');
        Self {
            pattern: pattern.trim_start_matches('$').chars().collect(),
            dollar,
        }
    }

    /// Whether the pattern is empty (everything matches).
    pub fn is_empty(&self) -> bool {
        self.pattern.is_empty()
    }

    /// How `name` matches, or `None` when it does not.
    pub fn matches(&self, name: &str) -> Option<NameMatch> {
        // `$na` only matches variables; a variable matches without its `$`.
        let offset = usize::from(name.starts_with('$'));
        if self.dollar && offset == 0 {
            return None;
        }
        let body = &name[offset..];
        let chars: Vec<char> = body.chars().collect();
        let bytes: Vec<usize> = body.char_indices().map(|(i, _)| i + offset).collect();
        let to_range = |from: usize, to: usize| {
            let end = bytes.get(to).copied().unwrap_or(name.len());
            bytes[from]..end
        };
        let p = &self.pattern;
        if p.is_empty() {
            return Some(NameMatch {
                tier: MatchTier::Prefix,
                fragments: Vec::new(),
            });
        }
        if p.len() <= chars.len()
            && chars[..p.len()]
                .iter()
                .zip(p)
                .all(|(a, b)| same_letter(*a, *b))
        {
            let exact_case = chars[..p.len()] == p[..];
            let tier = match (p.len() == chars.len(), exact_case) {
                (true, true) => MatchTier::Exact,
                (_, true) => MatchTier::Prefix,
                _ => MatchTier::PrefixIgnoringCase,
            };
            return Some(NameMatch {
                tier,
                fragments: vec![to_range(0, p.len())],
            });
        }
        let mut path = Vec::new();
        if same_letter(p[0], chars.first().copied()?) && self.humps(&chars, 0, 0, &mut path) {
            return Some(NameMatch {
                tier: MatchTier::Humps,
                fragments: merge(&path, &to_range),
            });
        }
        for start in (1..chars.len()).filter(|&i| is_word_start(&chars, i)) {
            path.clear();
            if same_letter(p[0], chars[start]) && self.humps(&chars, start, 0, &mut path) {
                return Some(NameMatch {
                    tier: MatchTier::Middle,
                    fragments: merge(&path, &to_range),
                });
            }
        }
        None
    }

    /// Matches pattern chars from `pi` onward at name positions from `ni`,
    /// recording matched name indices in `path`. A pattern char matches the
    /// next name char (continuing a fragment) or any later word start;
    /// uppercase pattern chars only match word starts or the same
    /// uppercase letter.
    fn humps(&self, chars: &[char], ni: usize, pi: usize, path: &mut Vec<usize>) -> bool {
        if pi == self.pattern.len() {
            return true;
        }
        let pc = self.pattern[pi];
        // The first pattern char is pinned to `ni` by the caller.
        if pi == 0 {
            path.push(ni);
            if self.humps(chars, ni + 1, 1, path) {
                return true;
            }
            path.pop();
            return false;
        }
        let continuing = path.last().is_some_and(|&last| last + 1 == ni);
        if ni < chars.len()
            && continuing
            && same_letter(pc, chars[ni])
            && (!pc.is_uppercase() || chars[ni] == pc || is_word_start(chars, ni))
        {
            path.push(ni);
            if self.humps(chars, ni + 1, pi + 1, path) {
                return true;
            }
            path.pop();
        }
        for next in ni..chars.len() {
            if (next != ni || !continuing)
                && is_word_start(chars, next)
                && same_letter(pc, chars[next])
            {
                path.push(next);
                if self.humps(chars, next + 1, pi + 1, path) {
                    return true;
                }
                path.pop();
            }
        }
        false
    }
}

/// Merges adjacent matched char indices into byte ranges.
fn merge(indices: &[usize], to_range: &impl Fn(usize, usize) -> Range<usize>) -> Vec<Range<usize>> {
    let mut out: Vec<(usize, usize)> = Vec::new();
    for &i in indices {
        match out.last_mut() {
            Some((_, end)) if *end == i => *end = i + 1,
            _ => out.push((i, i + 1)),
        }
    }
    out.into_iter().map(|(a, b)| to_range(a, b)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tier(pattern: &str, name: &str) -> Option<MatchTier> {
        Matcher::new(pattern).matches(name).map(|m| m.tier)
    }

    #[test]
    fn prefixes_should_rank_by_case_and_exactness() {
        assert_eq!(tier("get", "get"), Some(MatchTier::Exact));
        assert_eq!(tier("get", "getName"), Some(MatchTier::Prefix));
        assert_eq!(tier("Get", "getName"), Some(MatchTier::PrefixIgnoringCase));
        assert_eq!(tier("", "anything"), Some(MatchTier::Prefix));
    }

    #[test]
    fn camel_humps_should_match_word_starts() {
        assert_eq!(tier("gUN", "getUserName"), Some(MatchTier::Humps));
        assert_eq!(tier("gun", "getUserName"), Some(MatchTier::Humps));
        assert_eq!(tier("getUN", "getUserName"), Some(MatchTier::Humps));
        assert_eq!(tier("fb", "foo_bar"), Some(MatchTier::Humps));
        assert_eq!(tier("HP", "HTMLParser"), Some(MatchTier::Humps));
        assert_eq!(tier("usr", "getUserName"), None);
        assert_eq!(tier("gX", "getUserName"), None);
    }

    #[test]
    fn middle_matches_should_start_at_a_word() {
        assert_eq!(tier("Name", "getUserName"), Some(MatchTier::Middle));
        assert_eq!(tier("name", "getUserName"), Some(MatchTier::Middle));
        assert_eq!(tier("ser", "getUserName"), None);
    }

    #[test]
    fn variables_should_match_with_or_without_dollar() {
        assert_eq!(tier("$us", "$user"), Some(MatchTier::Prefix));
        assert_eq!(tier("us", "$user"), Some(MatchTier::Prefix));
        assert_eq!(tier("$us", "user"), None);
        assert_eq!(
            Matcher::new("$us").matches("$user").unwrap().fragments,
            vec![1..3]
        );
    }

    #[test]
    fn fragments_should_cover_matched_letters() {
        let m = Matcher::new("gUN").matches("getUserName").unwrap();
        assert_eq!(m.fragments, vec![0..1, 3..4, 7..8]);
        let m = Matcher::new("getUs").matches("getUserName").unwrap();
        assert_eq!(m.fragments, vec![0..5]);
    }
}
