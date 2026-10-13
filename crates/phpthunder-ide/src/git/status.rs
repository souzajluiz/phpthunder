//! Parsing `git status --porcelain=v2 --branch -z` and `git branch` output.

use super::*;
use std::path::Path;

/// Kind of change from a porcelain v2 `XY` pair.
fn change_from(xy: &str) -> Change {
    let mut chars = xy.chars();
    let (x, y) = (chars.next().unwrap_or('.'), chars.next().unwrap_or('.'));
    if x == 'D' || y == 'D' {
        Change::Deleted
    } else if x == 'A' {
        Change::Added
    } else if x == 'R' || x == 'C' {
        Change::Renamed
    } else {
        Change::Modified
    }
}

/// Parses NUL-separated porcelain v2 status of the repository at `root`.
pub fn parse_status(output: &str, root: &Path) -> RepoStatus {
    let mut status = RepoStatus::default();
    let mut records = output.split('\0').filter(|r| !r.is_empty());
    let file = |rel: &str, change: Change, staged: bool, unstaged: bool| FileChange {
        path: root.join(rel),
        rel: rel.to_string(),
        change,
        staged,
        unstaged,
    };
    while let Some(record) = records.next() {
        if let Some(header) = record.strip_prefix("# ") {
            if let Some(head) = header.strip_prefix("branch.head ") {
                status.branch = (head != "(detached)").then(|| head.to_string());
            } else if let Some(up) = header.strip_prefix("branch.upstream ") {
                status.upstream = Some(up.to_string());
            } else if let Some(ab) = header.strip_prefix("branch.ab ") {
                for part in ab.split_whitespace() {
                    if let Some(n) = part.strip_prefix('+') {
                        status.ahead = n.parse().unwrap_or(0);
                    } else if let Some(n) = part.strip_prefix('-') {
                        status.behind = n.parse().unwrap_or(0);
                    }
                }
            }
            continue;
        }
        let fields: Vec<&str> = record.splitn(9, ' ').collect();
        match fields.first().copied() {
            // Ordinary change: 1 XY sub mH mI mW hH hI path
            Some("1") if fields.len() == 9 => {
                let xy = fields[1];
                status.files.push(file(
                    fields[8],
                    change_from(xy),
                    !xy.starts_with('.'),
                    !xy.ends_with('.'),
                ));
            }
            // Rename or copy: 2 XY sub mH mI mW hH hI Xscore path, then the original path.
            Some("2") => {
                let parts: Vec<&str> = record.splitn(10, ' ').collect();
                if let Some(rel) = parts.get(9) {
                    let xy = parts[1];
                    status.files.push(file(
                        rel,
                        Change::Renamed,
                        !xy.starts_with('.'),
                        !xy.ends_with('.'),
                    ));
                }
                records.next();
            }
            // Unmerged: u XY sub m1 m2 m3 mW h1 h2 h3 path
            Some("u") => {
                if let Some(rel) = record.splitn(11, ' ').nth(10) {
                    status
                        .files
                        .push(file(rel, Change::Conflicted, false, true));
                }
            }
            Some("?") => {
                if let Some(rel) = record.get(2..) {
                    status.files.push(file(rel, Change::Untracked, false, true));
                }
            }
            _ => {}
        }
    }
    status.files.sort_by(|a, b| a.rel.cmp(&b.rel));
    status
}

/// Parses `git branch --all --format=%(HEAD)%09%(refname)` output.
pub fn parse_branches(output: &str) -> Vec<Branch> {
    let mut branches: Vec<Branch> = output
        .lines()
        .filter_map(|line| {
            let (head, refname) = line.split_once('\t')?;
            let (name, remote) = match refname.strip_prefix("refs/heads/") {
                Some(n) => (n, false),
                None => (refname.strip_prefix("refs/remotes/")?, true),
            };
            // `origin/HEAD` points at another remote branch.
            (!name.ends_with("/HEAD")).then(|| Branch {
                name: name.to_string(),
                current: head == "*",
                remote,
            })
        })
        .collect();
    branches.sort_by(|a, b| (a.remote, &a.name).cmp(&(b.remote, &b.name)));
    branches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_status_should_read_branch_and_changes() {
        let out = "# branch.oid abc\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -1\0\
1 .M N... 100644 100644 100644 a a src/User.php\0\
1 A. N... 000000 100644 100644 0 b new.php\0\
2 R. N... 100644 100644 100644 a a R100 moved.php\0old.php\0\
? notes.txt\0";
        let s = parse_status(out, Path::new("/repo"));
        assert_eq!(s.branch.as_deref(), Some("main"));
        assert_eq!((s.ahead, s.behind), (2, 1));
        let rels: Vec<&str> = s.files.iter().map(|f| f.rel.as_str()).collect();
        assert_eq!(rels, ["moved.php", "new.php", "notes.txt", "src/User.php"]);
        let user = &s.files[3];
        assert_eq!(
            (user.change, user.staged, user.unstaged),
            (Change::Modified, false, true)
        );
        assert_eq!(user.path, Path::new("/repo/src/User.php"));
        assert!(s.files[1].staged && s.files[1].change == Change::Added);
        assert_eq!(s.files[2].change, Change::Untracked);
    }

    #[test]
    fn parse_branches_should_split_local_and_remote() {
        let out = "*\trefs/heads/main\n \trefs/heads/feature\n \trefs/remotes/origin/HEAD\n \trefs/remotes/origin/main\n";
        let b = parse_branches(out);
        let names: Vec<(&str, bool, bool)> = b
            .iter()
            .map(|b| (b.name.as_str(), b.current, b.remote))
            .collect();
        assert_eq!(
            names,
            [
                ("feature", false, false),
                ("main", true, false),
                ("origin/main", false, true)
            ]
        );
    }
}
