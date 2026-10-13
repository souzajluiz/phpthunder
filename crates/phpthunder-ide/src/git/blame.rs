//! `git blame` for the Annotate view: who last changed each line, and when.

use std::collections::HashMap;

/// The last change to one line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineBlame {
    /// Author name; empty for lines not committed yet.
    pub author: String,
    /// Commit date as `YYYY-MM-DD`; empty for lines not committed yet.
    pub date: String,
    /// Commit summary line.
    pub summary: String,
}

/// Parses `git blame --porcelain` output into one entry per line of the
/// file, in order. Commit details appear only the first time a commit is
/// mentioned, so they are remembered by hash.
pub fn parse_blame(output: &str) -> Vec<LineBlame> {
    let mut commits: HashMap<String, LineBlame> = HashMap::new();
    let mut lines = Vec::new();
    let mut current: Option<String> = None;
    for line in output.lines() {
        if line.starts_with('\t') {
            if let Some(hash) = current.take() {
                let blame = commits.get(&hash).cloned().unwrap_or(LineBlame {
                    author: String::new(),
                    date: String::new(),
                    summary: String::new(),
                });
                lines.push(blame);
            }
            continue;
        }
        let mut parts = line.splitn(2, ' ');
        let (key, value) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        if key.len() == 40 && key.bytes().all(|b| b.is_ascii_hexdigit()) {
            let uncommitted = key.bytes().all(|b| b == b'0');
            commits.entry(key.to_string()).or_insert_with(|| LineBlame {
                author: if uncommitted {
                    String::new()
                } else {
                    "?".into()
                },
                date: String::new(),
                summary: String::new(),
            });
            current = Some(key.to_string());
            continue;
        }
        let Some(entry) = current.as_ref().and_then(|h| commits.get_mut(h)) else {
            continue;
        };
        if entry.author.is_empty() && entry.date.is_empty() && entry.summary.is_empty() {
            // Uncommitted: keep everything empty.
            continue;
        }
        match key {
            "author" => entry.author = value.to_string(),
            "author-time" => entry.date = value.parse().map(date_of).unwrap_or_default(),
            "summary" => entry.summary = value.to_string(),
            _ => {}
        }
    }
    lines
}

/// `YYYY-MM-DD` (UTC) of a Unix time in seconds.
fn date_of(secs: i64) -> String {
    // Days to civil date (Howard Hinnant's algorithm).
    let z = secs.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_of_should_format_a_known_day() {
        assert_eq!(date_of(1_700_000_000), "2023-11-14");
    }

    #[test]
    fn parse_blame_should_reuse_commit_details() {
        let hash = "a".repeat(40);
        let out = format!(
            "{hash} 1 1 2\nauthor Ana\nauthor-time 1700000000\nsummary Init\n\tline one\n{hash} 2 2\n\tline two\n"
        );
        let blame = parse_blame(&out);
        assert_eq!(blame[1].author, "Ana");
    }

    #[test]
    fn parse_blame_should_leave_uncommitted_lines_empty() {
        let out = format!(
            "{} 1 1 1\nauthor Not Committed Yet\n\tnew\n",
            "0".repeat(40)
        );
        assert_eq!(parse_blame(&out)[0].author, "");
    }
}
