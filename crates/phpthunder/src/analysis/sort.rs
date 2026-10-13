//! Ordering completion candidates. Candidates that do not match the typed
//! text are dropped; the rest are sorted by a chain of criteria, each one
//! only breaking ties of the previous:
//!
//! 1. match quality ([`MatchTier`]: exact, prefix, prefix ignoring case,
//!    camel humps, middle);
//! 2. how often the user picked the candidate before (usage statistics);
//! 3. proximity: names usable without an import before imported ones;
//! 4. keywords after symbols;
//! 5. shorter names first, then alphabetical.

use super::*;
use std::ops::Range;

/// `Candidate::detail` of PHP keyword proposals.
pub const KEYWORD_DETAIL: &str = "keyword";

/// A candidate that matched, with what matched.
#[derive(Clone, Debug)]
pub struct Ranked {
    /// The proposal.
    pub candidate: Candidate,
    /// Match quality.
    pub tier: MatchTier,
    /// Byte ranges of the label that matched, for highlighting.
    pub fragments: Vec<Range<usize>>,
}

/// Matches `candidates` against `prefix` and sorts the matches best first,
/// keeping at most `limit`. `uses` returns how many times a candidate was
/// picked before (0 when unknown). Duplicates (same label and insert text)
/// are dropped.
pub fn rank_candidates(
    prefix: &str,
    candidates: Vec<Candidate>,
    uses: impl Fn(&Candidate) -> u32,
    limit: usize,
) -> Vec<Ranked> {
    let matcher = Matcher::new(prefix);
    let mut seen = std::collections::HashSet::new();
    let mut ranked: Vec<(u32, Ranked)> = candidates
        .into_iter()
        .filter(|c| seen.insert((c.label.clone(), c.insert_text.clone())))
        .filter_map(|candidate| {
            let found = matcher.matches(&candidate.label)?;
            let count = uses(&candidate);
            Some((
                count,
                Ranked {
                    candidate,
                    tier: found.tier,
                    fragments: found.fragments,
                },
            ))
        })
        .collect();
    ranked.sort_by(|(ua, a), (ub, b)| {
        let keyword = |r: &Ranked| r.candidate.detail == KEYWORD_DETAIL;
        b.tier
            .cmp(&a.tier)
            .then(ub.cmp(ua))
            .then(
                a.candidate
                    .import
                    .is_some()
                    .cmp(&b.candidate.import.is_some()),
            )
            .then(keyword(a).cmp(&keyword(b)))
            .then(a.candidate.label.len().cmp(&b.candidate.label.len()))
            .then_with(|| {
                a.candidate
                    .label
                    .to_lowercase()
                    .cmp(&b.candidate.label.to_lowercase())
            })
    });
    ranked.truncate(limit);
    ranked.into_iter().map(|(_, r)| r).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symbol::Kind;

    fn candidate(label: &str) -> Candidate {
        Candidate {
            label: label.into(),
            kind: Kind::Method,
            detail: String::new(),
            insert_text: label.into(),
            import: None,
        }
    }

    fn labels(ranked: &[Ranked]) -> Vec<&str> {
        ranked.iter().map(|r| r.candidate.label.as_str()).collect()
    }

    #[test]
    fn rank_should_order_by_match_quality_then_length() {
        let all = ["getUserName", "get", "getAll", "targetName", "unrelated"]
            .map(candidate)
            .to_vec();
        let ranked = rank_candidates("get", all, |_| 0, 10);
        assert_eq!(labels(&ranked), ["get", "getAll", "getUserName"]);
        let all = ["getUserName", "userName", "getUser"]
            .map(candidate)
            .to_vec();
        assert_eq!(
            labels(&rank_candidates("uN", all, |_| 0, 10)),
            ["userName", "getUserName"]
        );
    }

    #[test]
    fn rank_should_prefer_frequently_picked_candidates() {
        let all = ["save", "saveQuietly"].map(candidate).to_vec();
        let ranked = rank_candidates("sa", all, |c| u32::from(c.label == "saveQuietly") * 3, 10);
        assert_eq!(labels(&ranked), ["saveQuietly", "save"]);
    }

    #[test]
    fn rank_should_put_imports_and_keywords_after_local_symbols() {
        let mut imported = candidate("Request");
        imported.import = Some((0, "use X\\Request;".into()));
        let mut keyword = candidate("return");
        keyword.detail = KEYWORD_DETAIL.into();
        let all = vec![imported, candidate("Resource")];
        assert_eq!(
            labels(&rank_candidates("Re", all, |_| 0, 10)),
            ["Resource", "Request"]
        );
        let all = vec![keyword, candidate("rewind")];
        assert_eq!(
            labels(&rank_candidates("re", all, |_| 0, 10)),
            ["rewind", "return"]
        );
    }
}
