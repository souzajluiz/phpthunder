//! Built-in HTML and Blade formatter: re-indents each line from the nesting
//! of HTML elements and, in Blade, of block directives (`@if … @endif`,
//! `@foreach`, `@section`, `@switch`/`@case`, …), treating `@else`-like
//! directives as a step back. Only leading whitespace changes, so a line is
//! never split or joined.
//!
//! Some regions keep the author's own relative indentation, shifted to
//! their depth: attributes of a start tag spanning lines, directive
//! arguments and `{{ }}` echoes spanning lines (`@props([...])`),
//! `<script>`, `<style>` and `@php` blocks. `<pre>`, `<textarea>` and
//! `@verbatim` content stays exactly as written. Children of `<html>` and
//! `<body>` are not indented, as is customary.

use std::collections::HashSet;

/// HTML elements that never have a closing tag.
const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr", "!doctype",
];

/// Elements whose children are not indented.
const FLAT: &[&str] = &["html", "body"];

/// Blade directives that open a block closed by `@end…` (or `@show`,
/// `@stop`, `@append`, `@overwrite` for sections).
const BLADE_OPEN: &[&str] = &[
    "if",
    "unless",
    "isset",
    "foreach",
    "forelse",
    "for",
    "while",
    "switch",
    "auth",
    "guest",
    "can",
    "cannot",
    "canany",
    "env",
    "production",
    "error",
    "section",
    "push",
    "prepend",
    "pushonce",
    "prependonce",
    "component",
    "slot",
    "once",
    "fragment",
    "session",
    "context",
    "hassection",
    "sectionmissing",
];

/// Directives that close the current block and open the next one.
const BLADE_ELSE: &[&str] = &[
    "else",
    "elseif",
    "elseauth",
    "elseguest",
    "elsecan",
    "elsecannot",
    "elsecanany",
    "elseenv",
];

/// Directives that end a section without `@endsection`.
const BLADE_SECTION_END: &[&str] = &["show", "stop", "append", "overwrite"];

/// A region whose lines are not re-indented from the nesting alone.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Region {
    /// Kept exactly as written until the closing marker.
    Verbatim(&'static str),
    /// Shifted as a whole to the current depth until the closing marker.
    Shifted {
        /// Line prefix that ends the region (`</script`).
        end: &'static str,
        /// Original indentation of the region's first line.
        base: Option<usize>,
    },
    /// The attributes of a start tag that did not end on its first line.
    Tag {
        /// Depth the tag's first line was printed at.
        depth: usize,
        /// Original indentation of the first attribute line.
        base: Option<usize>,
        /// Whether the tag's first line counted it as an opened element.
        counted: bool,
        /// Brackets of attribute expressions still open.
        open: usize,
        /// Column of the first attribute when it follows the name on the
        /// tag's first line (`<button type="…"`): later attributes align
        /// under it. `None` indents them one level instead.
        align: Option<usize>,
        /// Region the element starts once the tag ends (`<script …>`).
        then: Option<Box<Region>>,
    },
    /// Directive arguments or an echo spanning lines.
    Expr {
        /// Depth the expression's first line was printed at.
        depth: usize,
        /// Original indentation of the first continuation line.
        base: Option<usize>,
        /// Brackets still open, or `None` inside a `{{ }}` echo.
        open: Option<usize>,
    },
}

/// Nesting changes found on one line.
#[derive(Debug, Default)]
struct Scan {
    /// Closers before any opener at the start of the line (`</div>`,
    /// `@endif`, `@else`): the line itself is printed that much shallower.
    leading_closes: usize,
    /// Openers on the line (elements and block directives).
    opens: usize,
    /// Closers on the line, `leading_closes` included.
    closes: usize,
    /// `@else`-like directives at the start: printed one step back, depth
    /// unchanged.
    reopens: usize,
    /// `@case` / `@default` at the start of the line.
    case: bool,
    /// `@switch` directives opened on the line.
    switches: usize,
    /// `@endswitch` at the start of the line.
    end_switch: bool,
    /// A comment left open at the end of the line, by its end marker.
    open_comment: Option<&'static str>,
    /// A region starting after this line.
    region: Option<Region>,
}

/// Re-indents `src` with `indent` per level; `blade` also nests Blade
/// directives. Ends with a single `\n`. Never fails: unbalanced markup
/// only produces odd indentation, never lost text.
pub fn format(src: &str, indent: &str, blade: bool) -> String {
    let blocks = if blade {
        end_directives(src)
    } else {
        HashSet::new()
    };
    let mut out = String::with_capacity(src.len() + src.len() / 8);
    let mut depth: usize = 0;
    let mut region: Option<Region> = None;
    // An open multi-line comment: its end marker, the indentation of its
    // first line, and the depth that line was printed at.
    let mut comment: Option<(&'static str, usize, usize)> = None;
    // One entry per open `@switch`: whether a `@case` is open in it.
    let mut switches: Vec<bool> = Vec::new();
    for line in src.lines() {
        let trimmed = line.trim();
        let lead = line.len() - line.trim_start().len();
        match region.take() {
            Some(Region::Verbatim(end)) => {
                if find_ci(trimmed, end) == Some(0) {
                    depth = depth.saturating_sub(1);
                    push_line(&mut out, indent, depth, 0, trimmed);
                } else {
                    if find_ci(trimmed, end).is_some() {
                        depth = depth.saturating_sub(1);
                    } else {
                        region = Some(Region::Verbatim(end));
                    }
                    out.push_str(line.trim_end());
                    out.push('\n');
                }
                continue;
            }
            Some(Region::Shifted { end, mut base }) => {
                if find_ci(trimmed, end) == Some(0) {
                    depth = depth.saturating_sub(1);
                    push_line(&mut out, indent, depth, 0, trimmed);
                } else {
                    let closing = find_ci(trimmed, end).is_some();
                    if trimmed.is_empty() {
                        out.push('\n');
                    } else if closing && base.is_some_and(|b| lead < b) {
                        // Less indented than the content: back to the opener.
                        push_line(&mut out, indent, depth.saturating_sub(1), 0, trimmed);
                    } else {
                        let base = *base.get_or_insert(lead);
                        push_line(&mut out, indent, depth, lead.saturating_sub(base), trimmed);
                    }
                    // `) !!}</script>`: the region ends mid-line.
                    if find_ci(trimmed, end).is_some() {
                        depth = depth.saturating_sub(1);
                    } else {
                        region = Some(Region::Shifted { end, base });
                    }
                }
                continue;
            }
            Some(Region::Tag {
                depth: at,
                mut base,
                counted,
                open,
                align,
                then,
            }) => {
                if trimmed.is_empty() {
                    out.push('\n');
                    region = Some(Region::Tag {
                        depth: at,
                        base,
                        counted,
                        open,
                        align,
                        then,
                    });
                    continue;
                }
                // A line that starts with the tag's end goes back to the tag's column.
                // So does a line less indented than the attributes (`])>`).
                if trimmed.starts_with('>')
                    || trimmed.starts_with("/>")
                    || base.is_some_and(|b| lead < b)
                {
                    push_line(&mut out, indent, at, 0, trimmed);
                } else {
                    let base = *base.get_or_insert(lead);
                    let extra = lead.saturating_sub(base);
                    match align {
                        Some(col) => push_line(&mut out, indent, at, col + extra, trimmed),
                        None => push_line(&mut out, indent, at + 1, extra, trimmed),
                    }
                }
                match tag_close(trimmed, open) {
                    Err(open) => {
                        region = Some(Region::Tag {
                            depth: at,
                            base,
                            counted,
                            open,
                            align,
                            then,
                        });
                    }
                    Ok(end) => {
                        let rest = &trimmed[end..];
                        if counted && trimmed[..end].ends_with("/>") {
                            depth = depth.saturating_sub(1);
                        } else if let Some(next) = then {
                            match find_ci(rest, closing_of(&next)) {
                                Some(_) => depth = depth.saturating_sub(1),
                                None => region = Some(*next),
                            }
                            continue;
                        }
                        let scan = scan(rest, blade, &blocks);
                        depth = after(depth, &scan, &mut region, &mut switches, at);
                    }
                }
                continue;
            }
            Some(Region::Expr {
                depth: at,
                mut base,
                open,
            }) => {
                let closes = match open {
                    Some(n) => bracket_close(trimmed, n),
                    None => find_echo_end(trimmed),
                };
                // The line that closes the expression goes back to its column
                // when it starts with the closer (`])`, `}}`).
                let starts_closed = closes.is_some() && trimmed.starts_with([')', ']', '}']);
                if trimmed.is_empty() {
                    out.push('\n');
                } else if starts_closed {
                    push_line(&mut out, indent, at, 0, trimmed);
                } else {
                    let base = *base.get_or_insert(lead);
                    push_line(&mut out, indent, at + 1, lead.saturating_sub(base), trimmed);
                }
                match closes {
                    Some(end) => {
                        let scan = scan(&trimmed[end..], blade, &blocks);
                        depth = after(depth, &scan, &mut region, &mut switches, at);
                    }
                    None => {
                        let open = open.map(|n| n + bracket_delta(trimmed));
                        region = Some(Region::Expr {
                            depth: at,
                            base,
                            open,
                        });
                    }
                }
                continue;
            }
            None => {}
        }
        if trimmed.is_empty() {
            out.push('\n');
            continue;
        }
        if let Some((end, base, at)) = comment {
            push_line(&mut out, indent, at, lead.saturating_sub(base), trimmed);
            if trimmed.contains(end) {
                comment = None;
            }
            continue;
        }
        let mut scan = scan(trimmed, blade, &blocks);
        let mut print = depth.saturating_sub(scan.leading_closes);
        if scan.end_switch && switches.last() == Some(&true) {
            // `@endswitch` also closes the last `@case`.
            print = print.saturating_sub(1);
            scan.leading_closes += 1;
            scan.closes += 1;
        }
        if scan.case {
            match switches.last_mut() {
                Some(true) => {
                    print = print.saturating_sub(1);
                    scan.reopens += 1;
                }
                Some(open) => {
                    *open = true;
                    scan.opens += 1;
                }
                None => {}
            }
        }
        push_line(&mut out, indent, print, 0, trimmed);
        comment = scan.open_comment.map(|end| (end, lead, print));
        depth = after(depth, &scan, &mut region, &mut switches, print);
    }
    // Lines map one to one, so ignored ranges are restored afterwards.
    let ignored = ignored_lines(src);
    if ignored.iter().any(|&i| i) {
        out = out
            .lines()
            .zip(src.lines().chain(std::iter::repeat("")))
            .zip(ignored.iter().copied().chain(std::iter::repeat(false)))
            .map(|((formatted, original), ignore)| {
                if ignore {
                    original.trim_end()
                } else {
                    formatted
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        out.push('\n');
    }
    while out.ends_with("\n\n") {
        out.pop();
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Directive names that have a matching `@end…` somewhere in `src`
/// (`@capture` … `@endcapture` from a package), so they nest like the
/// built-in block directives.
fn end_directives(src: &str) -> HashSet<String> {
    src.match_indices("@end")
        .map(|(i, _)| directive_name(&src[i + 4..]))
        // `@php` / `@verbatim` open raw regions, handled on their own.
        .filter(|n| !n.is_empty() && n != "php" && n != "verbatim")
        .collect()
}

/// Per line of `src`, whether it lies between a `format-ignore-start` and
/// `format-ignore-end` marker (or `@formatter:off` / `@formatter:on`), in
/// any comment. Marker lines themselves are formatted.
fn ignored_lines(src: &str) -> Vec<bool> {
    let mut ignoring = false;
    src.lines()
        .map(|line| {
            if line.contains("format-ignore-end") || line.contains("@formatter:on") {
                ignoring = false;
                return false;
            }
            let this = ignoring;
            if line.contains("format-ignore-start") || line.contains("@formatter:off") {
                ignoring = true;
            }
            this
        })
        .collect()
}

/// New depth after a scanned line; records the region it starts (anchored
/// at `printed`, the depth the line was printed at) and its `@switch`es.
fn after(
    depth: usize,
    scan: &Scan,
    region: &mut Option<Region>,
    switches: &mut Vec<bool>,
    printed: usize,
) -> usize {
    switches.extend(std::iter::repeat_n(false, scan.switches));
    if scan.end_switch {
        switches.pop();
    }
    if let Some(r) = &scan.region {
        *region = Some(match r.clone() {
            Region::Tag {
                base,
                counted,
                open,
                align,
                then,
                ..
            } => Region::Tag {
                depth: printed,
                base,
                counted,
                open,
                align,
                then,
            },
            Region::Expr { base, open, .. } => Region::Expr {
                depth: printed,
                base,
                open,
            },
            other => other,
        });
    }
    (depth + scan.opens + scan.reopens).saturating_sub(scan.closes + scan.reopens)
}

/// Writes `text` indented `depth` levels plus `extra` spaces.
fn push_line(out: &mut String, indent: &str, depth: usize, extra: usize, text: &str) {
    out.push_str(&indent.repeat(depth));
    out.push_str(&" ".repeat(extra));
    out.push_str(text);
    out.push('\n');
}

/// ASCII case-insensitive `find`.
fn find_ci(s: &str, needle: &str) -> Option<usize> {
    s.to_ascii_lowercase().find(&needle.to_ascii_lowercase())
}

/// The marker that ends a region started by an element.
fn closing_of(region: &Region) -> &'static str {
    match region {
        Region::Verbatim(end) | Region::Shifted { end, .. } => end,
        _ => "",
    }
}

/// Where the tag that `s` is inside of ends: `Ok` with the byte offset just
/// past its `>`, or `Err` with the brackets still open when it goes on.
/// `open` brackets are open before `s` (attribute expressions spanning
/// lines, `@class([`). A `>` inside quotes, `{{ }}`, brackets, or in `=>` /
/// `->` does not end the tag.
fn tag_close(s: &str, mut open: usize) -> Result<usize, usize> {
    let mut quote = None;
    let mut echo = false;
    let mut prev = ' ';
    for (i, c) in s.char_indices() {
        match (quote, c) {
            _ if echo => echo = !s[i..].starts_with("}}"),
            (None, '{') if s[i..].starts_with("{{") => echo = true,
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '(' | '[') => open += 1,
            (None, ')' | ']') => open = open.saturating_sub(1),
            (None, '>') if open == 0 && prev != '=' && prev != '-' => return Ok(i + 1),
            _ => {}
        }
        prev = c;
    }
    Err(open)
}

/// Brackets (`(`, `[`, `{`) that `s` leaves open, quotes skipped.
fn bracket_delta(s: &str) -> usize {
    let (mut open, mut close) = (0usize, 0usize);
    let mut quote = None;
    for c in s.chars() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '(' | '[' | '{') => open += 1,
            (None, ')' | ']' | '}') => close += 1,
            _ => {}
        }
    }
    open.saturating_sub(close)
}

/// Where `s` brings `open` unclosed brackets back to zero: the byte offset
/// just past that closer, or `None` when they stay open.
fn bracket_close(s: &str, mut open: usize) -> Option<usize> {
    let mut quote = None;
    for (i, c) in s.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '(' | '[' | '{') => open += 1,
            (None, ')' | ']' | '}') => {
                open = open.saturating_sub(1);
                if open == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

/// End of a `{{ … }}` / `{!! … !!}` echo continued on this line.
fn find_echo_end(s: &str) -> Option<usize> {
    [("}}", 2), ("!!}", 3)]
        .into_iter()
        .filter_map(|(m, n)| s.find(m).map(|i| i + n))
        .min()
}

/// Lowercased element name at the start of `s` (after `<` or `</`). A
/// Blade-echoed name (`<{{ $tag }}`) reads as `{{`, an ordinary element.
fn element_name(s: &str) -> String {
    if s.starts_with("{{") {
        return "{{".into();
    }
    s.chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | ':' | '.' | '!'))
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Blade directive name at the start of `s` (after `@`), lowercased.
fn directive_name(s: &str) -> String {
    s.chars()
        .take_while(char::is_ascii_alphabetic)
        .collect::<String>()
        .to_ascii_lowercase()
}

/// Whether the directive's argument list (right after its name) has a
/// top-level comma: `@section('title', 'Home')` is inline, not a block.
fn has_second_argument(after_name: &str) -> bool {
    let Some(args) = after_name.trim_start().strip_prefix('(') else {
        return false;
    };
    let (mut depth, mut quote) = (0i32, None);
    for c in args.chars() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), c) if c == q => quote = None,
            (None, '(' | '[') => depth += 1,
            (None, ')' | ']') if depth == 0 => return false,
            (None, ')' | ']') => depth -= 1,
            (None, ',') if depth == 0 => return true,
            _ => {}
        }
    }
    false
}

/// The region an element starts, if any.
fn region_for(name: &str) -> Option<Region> {
    match name {
        "pre" => Some(Region::Verbatim("</pre")),
        "textarea" => Some(Region::Verbatim("</textarea")),
        "script" => Some(Region::Shifted {
            end: "</script",
            base: None,
        }),
        "style" => Some(Region::Shifted {
            end: "</style",
            base: None,
        }),
        _ => None,
    }
}

/// Scans one trimmed line for openers and closers.
fn scan(line: &str, blade: bool, blocks: &HashSet<String>) -> Scan {
    let mut s = Scan::default();
    let mut seen_opener = false;
    // Text or an echo seen: closers after it do not start the line.
    let mut content_seen = false;
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < line.len() {
        let rest = &line[i..];
        if blade && rest.starts_with("{{--") {
            match rest.find("--}}") {
                Some(end) => i += end + 4,
                None => {
                    s.open_comment = Some("--}}");
                    break;
                }
            }
            continue;
        }
        if blade && (rest.starts_with("{{") || rest.starts_with("{!!")) {
            content_seen = true;
            match find_echo_end(rest) {
                Some(end) => i += end,
                None => {
                    s.region = Some(Region::Expr {
                        depth: 0,
                        base: None,
                        open: None,
                    });
                    break;
                }
            }
            continue;
        }
        if rest.starts_with("<!--") {
            match rest.find("-->") {
                Some(end) => i += end + 3,
                None => {
                    s.open_comment = Some("-->");
                    break;
                }
            }
            continue;
        }
        if let Some(after) = rest.strip_prefix("</") {
            let name = element_name(after);
            if !name.is_empty() {
                if !FLAT.contains(&name.as_str()) {
                    s.closes += 1;
                    if !seen_opener && !content_seen {
                        s.leading_closes += 1;
                    }
                }
                i += tag_close(rest, 0).unwrap_or(rest.len());
                continue;
            }
        }
        if let Some(after) = rest.strip_prefix('<') {
            let name = element_name(after);
            if (!name.is_empty() && !name.starts_with('!')) || name == "!doctype" {
                let counts = !VOID.contains(&name.as_str()) && !FLAT.contains(&name.as_str());
                let end = match tag_close(rest, 0) {
                    Ok(end) => end,
                    Err(open) => {
                        if counts {
                            s.opens += 1;
                        }
                        // `<button type="…"`: the column of `type`.
                        let after_name = &after[name.len().min(after.len())..];
                        let gap = after_name.len() - after_name.trim_start().len();
                        // Brackets left open (`<span @class([`) mean the next
                        // lines continue an expression, not the attribute list.
                        let align = (name != "{{"
                            && open == 0
                            && !after_name.trim().is_empty()
                            && gap > 0)
                            .then(|| line[..i].chars().count() + 1 + name.chars().count() + gap);
                        s.region = Some(Region::Tag {
                            depth: 0,
                            base: None,
                            counted: counts,
                            open,
                            align,
                            then: region_for(&name).map(Box::new),
                        });
                        break;
                    }
                };
                if counts && !rest[..end].ends_with("/>") {
                    let after_tag = &rest[end..];
                    if let Some(region) = region_for(&name) {
                        // `<script>…</script>` on one line is inline.
                        if let Some(close) = find_ci(after_tag, closing_of(&region)) {
                            i += end + close;
                            continue;
                        }
                        s.opens += 1;
                        s.region = Some(region);
                        break;
                    }
                    s.opens += 1;
                    seen_opener = true;
                }
                i += end;
                continue;
            }
        }
        if blade && bytes[i] == b'@' && (i == 0 || !bytes[i - 1].is_ascii_alphanumeric()) {
            if rest.starts_with("@@") {
                i += 2;
                continue;
            }
            let name = directive_name(&rest[1..]);
            let after_name = &rest[1 + name.len()..];
            let at_start = !seen_opener && !content_seen && s.closes == s.leading_closes;
            let has_args = after_name.trim_start().starts_with('(');
            if let Some(stem) = name.strip_prefix("end") {
                s.closes += 1;
                if at_start {
                    s.leading_closes += 1;
                }
                s.end_switch |= stem == "switch" && at_start;
            } else if BLADE_SECTION_END.contains(&name.as_str()) {
                s.closes += 1;
                if at_start {
                    s.leading_closes += 1;
                }
            } else if BLADE_ELSE.contains(&name.as_str()) || (name == "empty" && !has_args) {
                if at_start {
                    s.reopens += 1;
                    s.leading_closes += 1;
                }
            } else if matches!(name.as_str(), "case" | "default") {
                s.case |= at_start;
            } else if name == "verbatim" {
                s.opens += 1;
                s.region = Some(Region::Verbatim("@endverbatim"));
                break;
            } else if name == "php" && !has_args {
                s.opens += 1;
                s.region = Some(Region::Shifted {
                    end: "@endphp",
                    base: None,
                });
                break;
            } else if BLADE_OPEN.contains(&name.as_str())
                || blocks.contains(&name)
                || (name == "empty" && has_args)
            {
                let inline = matches!(name.as_str(), "section" | "slot" | "push" | "prepend")
                    && has_second_argument(after_name);
                if !inline {
                    s.opens += 1;
                    seen_opener = true;
                    s.switches += usize::from(name == "switch");
                }
            }
            i += 1 + name.len();
            // Arguments continued on the next lines (`@props([`).
            if has_args {
                let args_at = i + (after_name.len() - after_name.trim_start().len());
                match bracket_close(&line[args_at..], 0) {
                    Some(end) => i = args_at + end,
                    None => {
                        let open = bracket_delta(&line[args_at..]);
                        s.region = Some(Region::Expr {
                            depth: 0,
                            base: None,
                            open: Some(open),
                        });
                        break;
                    }
                }
            }
            continue;
        }
        let c = rest.chars().next().unwrap_or(' ');
        content_seen |= !c.is_whitespace();
        i += c.len_utf8();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_should_nest_elements_and_blade_blocks() {
        let src = "@extends('layouts.app')\n@section('title', 'Home')\n@section('content')\n<div class=\"card\">\n@if ($user)\n<p>{{ $user->name }}</p>\n@else\n<p>Guest</p>\n@endif\n@foreach ($items as $item)\n<li>{{ $item }}<br></li>\n@endforeach\n</div>\n@endsection\n";
        assert_eq!(
            format(src, "    ", true),
            "@extends('layouts.app')\n@section('title', 'Home')\n@section('content')\n    <div class=\"card\">\n        @if ($user)\n            <p>{{ $user->name }}</p>\n        @else\n            <p>Guest</p>\n        @endif\n        @foreach ($items as $item)\n            <li>{{ $item }}<br></li>\n        @endforeach\n    </div>\n@endsection\n"
        );
    }

    #[test]
    fn format_should_keep_relative_indentation_of_multi_line_tags() {
        let src = "<form>\n<input\n  type=\"text\"\n  @class([\n      'a' => $b,\n  ])\n>\n<x-button\n  label=\"Go\"\n/>\n</form>";
        assert_eq!(
            format(src, "  ", true),
            "<form>\n  <input\n    type=\"text\"\n    @class([\n        'a' => $b,\n    ])\n  >\n  <x-button\n    label=\"Go\"\n  />\n</form>\n"
        );
    }

    #[test]
    fn format_should_keep_multi_line_arguments_and_echoes() {
        let src = "@props([\n    'rows' => $rows,\n    'title' => 'X',\n])\n<div>\n{{ $a\n    ? 'yes'\n    : 'no' }}\n</div>";
        assert_eq!(
            format(src, "    ", true),
            "@props([\n    'rows' => $rows,\n    'title' => 'X',\n])\n<div>\n    {{ $a\n        ? 'yes'\n        : 'no' }}\n</div>\n"
        );
    }

    #[test]
    fn format_should_align_attributes_under_the_first_one() {
        let src = "<div>\n<button type=\"button\"\n   @click=\"go\"\n   class=\"b\">\nGo\n</button>\n</div>";
        assert_eq!(
            format(src, "    ", true),
            "<div>\n    <button type=\"button\"\n            @click=\"go\"\n            class=\"b\">\n        Go\n    </button>\n</div>\n"
        );
    }

    #[test]
    fn format_should_handle_real_world_corner_cases() {
        let src = "<div>\n    {{--\n        Note: keep\n            this shape.\n    --}}\n<script>{!! json_encode(\n    $data\n) !!}</script>\n<span @class([\n    'a' => $b,\n])>x</span>\n<li>Long text\n    goes on</li>\n</div>";
        assert_eq!(
            format(src, "  ", true),
            "<div>\n  {{--\n      Note: keep\n          this shape.\n  --}}\n  <script>{!! json_encode(\n    $data\n  ) !!}</script>\n  <span @class([\n    'a' => $b,\n  ])>x</span>\n  <li>Long text\n    goes on</li>\n</div>\n"
        );
    }

    #[test]
    fn format_should_nest_switch_cases() {
        let src =
            "@switch($s)\n@case('a')\n<b>A</b>\n@break\n@default\nnone\n@endswitch\n<p>after</p>";
        assert_eq!(
            format(src, "  ", true),
            "@switch($s)\n  @case('a')\n    <b>A</b>\n    @break\n  @default\n    none\n@endswitch\n<p>after</p>\n"
        );
    }

    #[test]
    fn format_should_shift_scripts_and_keep_pre_verbatim() {
        let src = "<div>\n<script>\n      if (a) {\n          b();\n      }\n</script>\n<pre>\n  keep   this\n</pre>\n</div>";
        assert_eq!(
            format(src, "  ", false),
            "<div>\n  <script>\n    if (a) {\n        b();\n    }\n  </script>\n  <pre>\n  keep   this\n  </pre>\n</div>\n"
        );
    }

    #[test]
    fn format_should_not_indent_children_of_html_and_body() {
        let src = "<html>\n<body>\n<p>x</p>\n</body>\n</html>";
        assert_eq!(
            format(src, "  ", false),
            "<html>\n<body>\n<p>x</p>\n</body>\n</html>\n"
        );
    }

    #[test]
    fn format_should_treat_echoed_tag_names_as_elements() {
        let src = "<{{ $tag }}\n    @if ($a)\n        href=\"x\"\n    @endif\n>\n<span>{{ $label }}</span>\n</{{ $tag }}>";
        assert_eq!(
            format(src, "    ", true),
            "<{{ $tag }}\n    @if ($a)\n        href=\"x\"\n    @endif\n>\n    <span>{{ $label }}</span>\n</{{ $tag }}>\n"
        );
    }

    #[test]
    fn format_should_nest_package_directives_and_honour_ignore_markers() {
        let src = "@php($x = 1)\n@php\n$y = 2;\n@endphp\n@capture($c)\n<b>x</b>\n@endcapture\n{{-- format-ignore-start --}}\n<p>\n      odd\n</p>\n{{-- format-ignore-end --}}\n<div>\n<i></i>\n</div>";
        assert_eq!(
            format(src, "  ", true),
            "@php($x = 1)\n@php\n  $y = 2;\n@endphp\n@capture($c)\n  <b>x</b>\n@endcapture\n{{-- format-ignore-start --}}\n<p>\n      odd\n</p>\n{{-- format-ignore-end --}}\n<div>\n  <i></i>\n</div>\n"
        );
    }

    #[test]
    fn format_should_skip_echoes_comments_and_emails() {
        let src = "<div>\n{{-- @if this is a comment --}}\n<a href=\"mailto:a@if.com\">{{ '<b>' }}</a>\n</div>";
        assert_eq!(
            format(src, "  ", true),
            "<div>\n  {{-- @if this is a comment --}}\n  <a href=\"mailto:a@if.com\">{{ '<b>' }}</a>\n</div>\n"
        );
    }
}
