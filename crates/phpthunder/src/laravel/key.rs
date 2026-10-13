//! Recognising a string argument that names a route, view or config key.
//! Works on plain text, so it serves PHP and Blade files alike.

use super::*;

/// Calls whose first argument names a key, by the text right before `(`.
const CALLS: &[(&str, KeyKind)] = &[
    ("route", KeyKind::Route),
    ("to_route", KeyKind::Route),
    ("routeIs", KeyKind::Route),
    ("URL::route", KeyKind::Route),
    ("Route::has", KeyKind::Route),
    ("view", KeyKind::View),
    ("View::make", KeyKind::View),
    ("View::exists", KeyKind::View),
    ("@include", KeyKind::View),
    ("@includeIf", KeyKind::View),
    ("@extends", KeyKind::View),
    ("@each", KeyKind::View),
    ("@component", KeyKind::View),
    ("config", KeyKind::Config),
    ("Config::get", KeyKind::Config),
    ("Config::string", KeyKind::Config),
    ("Config::integer", KeyKind::Config),
    ("Config::boolean", KeyKind::Config),
    ("Config::array", KeyKind::Config),
];

/// The key reference whose string contains `offset`, when the string is
/// the first argument of a known call (`route('…')`, `->view('…')`,
/// `@include('…')`).
pub fn key_at(src: &str, offset: usize) -> Option<KeyRef> {
    let line_start = src.get(..offset)?.rfind('\n').map_or(0, |i| i + 1);
    let line_end = src[offset..].find('\n').map_or(src.len(), |i| offset + i);
    let line = &src[line_start..line_end];
    let caret = offset - line_start;
    // Find the quoted string around the caret on this line.
    let mut open: Option<(usize, char)> = None;
    let mut found = None;
    for (i, c) in line.char_indices() {
        match open {
            None if c == '\'' || c == '"' => open = Some((i, c)),
            Some((start, quote)) if c == quote && !line[..i].ends_with('\\') => {
                if (start + 1..=i).contains(&caret) {
                    found = Some((start, i));
                    break;
                }
                open = None;
            }
            _ => {}
        }
    }
    let (quote_at, close_at) = match (found, open) {
        (Some(f), _) => f,
        // Unterminated string being typed: runs to the end of the line.
        (None, Some((start, _))) if caret > start => (start, line.len()),
        _ => return None,
    };
    let before = line[..quote_at].trim_end().strip_suffix('(')?.trim_end();
    let kind = CALLS.iter().find_map(|(call, kind)| {
        let rest = before.strip_suffix(call)?;
        let boundary = rest
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_' || c == '\\' || c == '$'));
        let method = rest.ends_with("->") || rest.ends_with("?->");
        (boundary || method).then_some(*kind)
    })?;
    let start = line_start + quote_at + 1;
    let end = line_start + close_at;
    Some(KeyRef {
        kind,
        start,
        end,
        prefix: src[start..offset].to_string(),
        value: src[start..end].to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(src: &str) -> Option<KeyRef> {
        let offset = src.find('|').unwrap();
        let src = src.replacen('|', "", 1);
        key_at(&src, offset)
    }

    #[test]
    fn key_at_should_recognise_calls_methods_and_directives() {
        let r = at("return redirect()->route('users.sh|ow');").unwrap();
        assert_eq!(
            (r.kind, r.prefix.as_str(), r.value.as_str()),
            (KeyKind::Route, "users.sh", "users.show")
        );
        assert_eq!(at("view('home|')").unwrap().kind, KeyKind::View);
        assert_eq!(at("@include('partials.n|av')").unwrap().kind, KeyKind::View);
        assert_eq!(
            at("$x = config(\"app.na|\");").unwrap().kind,
            KeyKind::Config
        );
        assert_eq!(at("Config::get('app|').").unwrap().kind, KeyKind::Config);
    }

    #[test]
    fn key_at_should_accept_an_unterminated_string() {
        let r = at("route('adm|").unwrap();
        assert_eq!(r.prefix, "adm");
    }

    #[test]
    fn key_at_should_ignore_other_strings() {
        assert_eq!(at("echo 'route(|x)';"), None);
        assert_eq!(at("myroute('a|')"), None);
        assert_eq!(at("route($name, 'a|')"), None);
        assert_eq!(at("route('a', 'b|')"), None);
    }
}
