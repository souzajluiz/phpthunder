//! Navigation for frameworks beyond Laravel: Twig template names (Symfony,
//! Drupal, Craft) to their files, and WordPress hooks between the code that
//! fires them (`do_action`, `apply_filters`) and the code that listens
//! (`add_action`, `add_filter`).

use std::path::{Path, PathBuf};

/// The quoted string literal on the caret's line that contains `offset`:
/// its content's byte range and text.
pub fn string_at(text: &str, offset: usize) -> Option<(usize, &str)> {
    let offset = offset.min(text.len());
    let line_start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[offset..].find('\n').map_or(text.len(), |i| offset + i);
    let line = &text[line_start..line_end];
    let mut i = 0;
    let b = line.as_bytes();
    while i < b.len() {
        let quote = b[i];
        if quote != b'\'' && quote != b'"' {
            i += 1;
            continue;
        }
        let mut end = i + 1;
        while end < b.len() && b[end] != quote {
            end += if b[end] == b'\\' { 2 } else { 1 };
        }
        let end = end.min(b.len());
        let (start, stop) = (line_start + i + 1, line_start + end);
        if (start..=stop).contains(&offset) {
            return Some((start, &text[start..stop]));
        }
        i = end + 1;
    }
    None
}

/// The Twig template named by the string at `offset`
/// (`'blog/show.html.twig'`, `'@App/mail.twig'`).
pub fn twig_name_at(text: &str, offset: usize) -> Option<&str> {
    let (_, name) = string_at(text, offset)?;
    name.ends_with(".twig").then_some(name)
}

/// Files that a Twig template name points at: `templates/<name>` (and
/// the Symfony 3 `app/Resources/views`), a `@Bundle/` name in the bundle's
/// or the overriding `templates/bundles/` folder, and otherwise any project
/// file ending in `/<name>` (Drupal themes and modules). Nearest first.
pub fn resolve_twig(root: &Path, files: &[PathBuf], name: &str) -> Vec<PathBuf> {
    let name = name.trim_start_matches('/');
    let (bundle, rest) = match name.strip_prefix('@').and_then(|n| n.split_once('/')) {
        Some((bundle, rest)) => (Some(bundle), rest),
        None => (None, name),
    };
    let mut found: Vec<PathBuf> = match bundle {
        Some(bundle) => vec![
            root.join("templates/bundles")
                .join(format!("{bundle}Bundle"))
                .join(rest),
            root.join("templates/bundles").join(bundle).join(rest),
            root.join("templates").join(rest),
        ],
        None => vec![
            root.join("templates").join(rest),
            root.join("app/Resources/views").join(rest),
        ],
    }
    .into_iter()
    .filter(|p| p.is_file())
    .collect();
    if found.is_empty() {
        let suffix = format!("/{rest}");
        found = files
            .iter()
            .filter(|p| p.to_string_lossy().replace('\\', "/").ends_with(&suffix))
            .cloned()
            .collect();
    }
    found
}

/// Which side of a WordPress hook a call is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HookSide {
    /// `add_action`, `add_filter`, `remove_*`, `has_*`: code that listens.
    Listen,
    /// `do_action`, `apply_filters` and their variants: code that fires.
    Fire,
}

/// Functions taking a hook name as their first argument, by side.
const HOOK_CALLS: &[(&str, HookSide)] = &[
    ("add_action", HookSide::Listen),
    ("add_filter", HookSide::Listen),
    ("remove_action", HookSide::Listen),
    ("remove_filter", HookSide::Listen),
    ("has_action", HookSide::Listen),
    ("has_filter", HookSide::Listen),
    ("do_action", HookSide::Fire),
    ("do_action_ref_array", HookSide::Fire),
    ("do_action_deprecated", HookSide::Fire),
    ("apply_filters", HookSide::Fire),
    ("apply_filters_ref_array", HookSide::Fire),
    ("apply_filters_deprecated", HookSide::Fire),
];

/// The hook call whose first argument starts at `quote` (the opening
/// quote), if the text before it is `name(` for a hook function.
fn hook_call_before(text: &str, quote: usize) -> Option<HookSide> {
    let before = text[..quote].trim_end();
    let before = before.strip_suffix('(')?.trim_end();
    let name_start = before
        .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .map_or(0, |i| i + 1);
    let name = &before[name_start..];
    HOOK_CALLS
        .iter()
        .find(|(call, _)| call.eq_ignore_ascii_case(name))
        .map(|(_, side)| *side)
}

/// The hook named by the string at `offset` when it is the first argument
/// of a hook function, with the side of that call.
pub fn hook_at(text: &str, offset: usize) -> Option<(&str, HookSide)> {
    let (start, name) = string_at(text, offset)?;
    let side = hook_call_before(text, start.checked_sub(1)?)?;
    Some((name, side))
}

/// Byte offsets of every call on `side` for `hook` in `text`.
pub fn hook_sites(text: &str, hook: &str, side: HookSide) -> Vec<usize> {
    let mut out = Vec::new();
    for quote in ['\'', '"'] {
        let needle = format!("{quote}{hook}{quote}");
        for (at, _) in text.match_indices(&needle) {
            if hook_call_before(text, at) == Some(side) {
                out.push(at + 1);
            }
        }
    }
    out.sort_unstable();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_at_should_find_the_literal_around_the_caret() {
        let text = "$this->render('blog/show.html.twig', []);";
        let at = text.find("show").unwrap();
        assert_eq!(twig_name_at(text, at), Some("blog/show.html.twig"));
    }

    #[test]
    fn strings_that_are_not_templates_should_be_ignored() {
        let text = "echo 'hello';";
        assert_eq!(twig_name_at(text, 7), None);
    }

    #[test]
    fn hooks_should_know_their_side() {
        let text = "add_action( 'init', 'boot' ); do_action('init');";
        let listen = text.find("init").unwrap();
        let fire = text.rfind("init").unwrap();
        assert_eq!(
            [hook_at(text, listen), hook_at(text, fire)],
            [
                Some(("init", HookSide::Listen)),
                Some(("init", HookSide::Fire))
            ]
        );
    }

    #[test]
    fn hook_sites_should_list_each_matching_call() {
        let text = "do_action('init');\napply_filters(\"init\", 1);\nadd_filter('init', 'f');";
        assert_eq!(hook_sites(text, "init", HookSide::Fire).len(), 2);
    }

    #[test]
    fn twig_names_should_fall_back_to_any_matching_file() {
        let files = vec![PathBuf::from(
            "/p/web/themes/olivero/templates/node.html.twig",
        )];
        assert_eq!(
            resolve_twig(Path::new("/p"), &files, "node.html.twig"),
            files
        );
    }
}
