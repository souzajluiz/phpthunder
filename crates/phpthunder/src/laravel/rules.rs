//! Validation rules: the names Laravel's validator accepts, and spotting
//! the rule being typed inside a rule string (`'required|em'`) or a rule
//! list (`['required', 'em']`) of a `rules()` method, `validate([...])` or
//! `Validator::make(...)`.

use super::*;
use std::path::PathBuf;

/// Rule names with their parameter syntax, as shown next to the name.
const RULES: &[(&str, &str)] = &[
    ("accepted", ""),
    ("accepted_if", "anotherfield,value"),
    ("active_url", ""),
    ("after", "date"),
    ("after_or_equal", "date"),
    ("alpha", ""),
    ("alpha_dash", ""),
    ("alpha_num", ""),
    ("any_of", ""),
    ("array", "key,…"),
    ("ascii", ""),
    ("bail", ""),
    ("before", "date"),
    ("before_or_equal", "date"),
    ("between", "min,max"),
    ("boolean", ""),
    ("confirmed", ""),
    ("contains", "foo,bar,…"),
    ("current_password", "guard"),
    ("date", ""),
    ("date_equals", "date"),
    ("date_format", "format,…"),
    ("decimal", "min,max"),
    ("declined", ""),
    ("declined_if", "anotherfield,value"),
    ("different", "field"),
    ("digits", "value"),
    ("digits_between", "min,max"),
    ("dimensions", "min_width=…"),
    ("distinct", ""),
    ("doesnt_start_with", "foo,bar,…"),
    ("doesnt_end_with", "foo,bar,…"),
    ("email", "rfc,dns,…"),
    ("ends_with", "foo,bar,…"),
    ("enum", ""),
    ("exclude", ""),
    ("exclude_if", "anotherfield,value"),
    ("exclude_unless", "anotherfield,value"),
    ("exclude_with", "anotherfield"),
    ("exclude_without", "anotherfield"),
    ("exists", "table,column"),
    ("extensions", "foo,…"),
    ("file", ""),
    ("filled", ""),
    ("gt", "field"),
    ("gte", "field"),
    ("hex_color", ""),
    ("image", ""),
    ("in", "foo,bar,…"),
    ("in_array", "anotherfield.*"),
    ("integer", ""),
    ("ip", ""),
    ("ipv4", ""),
    ("ipv6", ""),
    ("json", ""),
    ("list", ""),
    ("lowercase", ""),
    ("lt", "field"),
    ("lte", "field"),
    ("mac_address", ""),
    ("max", "value"),
    ("max_digits", "value"),
    ("mimetypes", "text/plain,…"),
    ("mimes", "foo,…"),
    ("min", "value"),
    ("min_digits", "value"),
    ("missing", ""),
    ("missing_if", "anotherfield,value"),
    ("missing_unless", "anotherfield,value"),
    ("missing_with", "foo,…"),
    ("missing_with_all", "foo,…"),
    ("multiple_of", "value"),
    ("not_in", "foo,bar,…"),
    ("not_regex", "pattern"),
    ("nullable", ""),
    ("numeric", ""),
    ("present", ""),
    ("present_if", "anotherfield,value"),
    ("present_unless", "anotherfield,value"),
    ("present_with", "foo,…"),
    ("present_with_all", "foo,…"),
    ("prohibited", ""),
    ("prohibited_if", "anotherfield,value"),
    ("prohibited_unless", "anotherfield,value"),
    ("prohibits", "anotherfield,…"),
    ("regex", "pattern"),
    ("required", ""),
    ("required_if", "anotherfield,value"),
    ("required_if_accepted", "anotherfield"),
    ("required_if_declined", "anotherfield"),
    ("required_unless", "anotherfield,value"),
    ("required_with", "foo,…"),
    ("required_with_all", "foo,…"),
    ("required_without", "foo,…"),
    ("required_without_all", "foo,…"),
    ("required_array_keys", "foo,…"),
    ("same", "field"),
    ("size", "value"),
    ("sometimes", ""),
    ("starts_with", "foo,bar,…"),
    ("string", ""),
    ("timezone", ""),
    ("unique", "table,column"),
    ("uppercase", ""),
    ("url", ""),
    ("ulid", ""),
    ("uuid", ""),
];

/// Text that opens a validation rules array; the caret must be inside an
/// array opened after the nearest one.
const RULE_OPENERS: &[&str] = &[
    "function rules(",
    "validate(",
    "validateWithBag(",
    "Validator::make(",
    "validator(",
    "$rules",
];

/// How far back (bytes) to look for a rules opener.
const RULE_LOOKBACK: usize = 8000;

/// Every rule as a key, for completion (detail: parameter syntax).
pub(super) fn validation_rules() -> Vec<LaravelKey> {
    RULES
        .iter()
        .map(|(name, params)| LaravelKey {
            name: (*name).to_string(),
            path: PathBuf::new(),
            pos: crate::syntax::Pos::default(),
            detail: if params.is_empty() {
                "rule".to_string()
            } else {
                format!("{name}:{params}")
            },
        })
        .collect()
}

/// Whether `name` is a rule Laravel knows.
pub fn is_validation_rule(name: &str) -> bool {
    RULES.iter().any(|(rule, _)| *rule == name)
}

/// The rule name being typed in the string `[content_start, content_end)`
/// at `offset`, when that string sits inside a rules array: the part
/// between the previous `|` and the next `|` or `:`.
pub(super) fn rule_at(
    src: &str,
    content_start: usize,
    content_end: usize,
    offset: usize,
) -> Option<KeyRef> {
    if !in_rules_array(src, content_start) {
        return None;
    }
    let content = &src[content_start..content_end];
    let caret = offset - content_start;
    let segment_start = content[..caret].rfind('|').map_or(0, |i| i + 1);
    // Typing a parameter (`max:2|`) is not typing a rule name.
    if content[segment_start..caret].contains(':') {
        return None;
    }
    let segment_end = content[segment_start..]
        .find(['|', ':'])
        .map_or(content.len(), |i| segment_start + i);
    let start = content_start + segment_start;
    Some(KeyRef {
        kind: KeyKind::ValidationRule,
        start,
        end: content_start + segment_end,
        prefix: src[start..offset].to_string(),
        value: content[segment_start..segment_end].to_string(),
    })
}

/// Whether byte `at` is inside a `[...]` opened after the nearest rules
/// opener (`rules()`, `validate(`, `Validator::make(`, `$rules`).
pub(super) fn in_rules_array(src: &str, at: usize) -> bool {
    let from = at.saturating_sub(RULE_LOOKBACK);
    let from = (from..=at).find(|i| src.is_char_boundary(*i)).unwrap_or(at);
    let window = &src[from..at];
    let Some(opener) = RULE_OPENERS
        .iter()
        .filter_map(|o| window.rfind(o).map(|i| i + o.len()))
        .max()
    else {
        return false;
    };
    // Bracket depth from the opener to the string: inside an array when
    // more `[` than `]`. Strings are skipped so `'['` does not count.
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut prev = '\0';
    for c in window[opener..].chars() {
        match quote {
            Some(q) if c == q && prev != '\\' => quote = None,
            Some(_) => {}
            None => match c {
                '\'' | '"' => quote = Some(c),
                '[' => depth += 1,
                ']' => depth -= 1,
                _ => {}
            },
        }
        prev = c;
    }
    depth > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(src: &str) -> Option<KeyRef> {
        let caret = src.find('^')?;
        let src = src.replacen('^', "", 1);
        let open = src[..caret].rfind('\'')?;
        let close = src[caret..].find('\'').map_or(src.len(), |i| caret + i);
        rule_at(&src, open + 1, close, caret)
    }

    #[test]
    fn rule_at_should_find_the_segment_in_a_rules_method() {
        let r = rule("public function rules() { return ['email' => 'required|em^ail|max:255']; }")
            .unwrap();
        assert_eq!((r.prefix.as_str(), r.value.as_str()), ("em", "email"));
    }

    #[test]
    fn rule_at_should_ignore_parameters() {
        assert!(rule("$request->validate(['a' => 'required|max:2^5|x']);").is_none());
    }

    #[test]
    fn rule_at_should_ignore_strings_outside_rules() {
        assert!(rule("echo 'required|em^';").is_none());
    }

    #[test]
    fn is_validation_rule_should_know_builtin_rules() {
        assert!(is_validation_rule("required_if"));
        assert!(!is_validation_rule("requird"));
    }
}
