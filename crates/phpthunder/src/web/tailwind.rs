//! Tailwind CSS class completion inside `class` attributes: the utility
//! classes of Tailwind's default theme (spacing, sizing, colors, layout,
//! typography, effects) behind any variant prefix (`hover:`, `md:`,
//! `dark:`), generated once from the theme scales.

use crate::analysis::Candidate;
use crate::symbol::Kind;
use std::sync::OnceLock;

/// Most proposals returned for one prefix.
const LIMIT: usize = 300;

/// Variants offered before a class.
const VARIANTS: &[&str] = &[
    "hover",
    "focus",
    "focus-visible",
    "focus-within",
    "active",
    "visited",
    "disabled",
    "checked",
    "required",
    "invalid",
    "first",
    "last",
    "odd",
    "even",
    "group-hover",
    "peer-hover",
    "placeholder",
    "before",
    "after",
    "dark",
    "sm",
    "md",
    "lg",
    "xl",
    "2xl",
    "motion-safe",
    "motion-reduce",
    "print",
    "rtl",
    "ltr",
];

/// The spacing scale.
const SPACING: &[&str] = &[
    "0", "px", "0.5", "1", "1.5", "2", "2.5", "3", "3.5", "4", "5", "6", "7", "8", "9", "10", "11",
    "12", "14", "16", "20", "24", "28", "32", "36", "40", "44", "48", "52", "56", "60", "64", "72",
    "80", "96",
];

/// Utilities taking the spacing scale (margins may also be negative).
const SPACED: &[&str] = &[
    "p",
    "px",
    "py",
    "pt",
    "pr",
    "pb",
    "pl",
    "ps",
    "pe",
    "m",
    "mx",
    "my",
    "mt",
    "mr",
    "mb",
    "ml",
    "ms",
    "me",
    "gap",
    "gap-x",
    "gap-y",
    "space-x",
    "space-y",
    "inset",
    "inset-x",
    "inset-y",
    "top",
    "right",
    "bottom",
    "left",
    "start",
    "end",
    "w",
    "h",
    "size",
    "min-w",
    "min-h",
    "max-h",
    "translate-x",
    "translate-y",
    "scroll-m",
    "scroll-p",
    "indent",
];

/// Size keywords for widths and heights.
const SIZES: &[&str] = &[
    "auto", "full", "screen", "svw", "svh", "dvh", "min", "max", "fit", "1/2", "1/3", "2/3", "1/4",
    "3/4", "1/5", "1/6",
];

/// Color families of the default palette.
const COLORS: &[&str] = &[
    "slate", "gray", "zinc", "neutral", "stone", "red", "orange", "amber", "yellow", "lime",
    "green", "emerald", "teal", "cyan", "sky", "blue", "indigo", "violet", "purple", "fuchsia",
    "pink", "rose",
];

/// Shades of each color family.
const SHADES: &[&str] = &[
    "50", "100", "200", "300", "400", "500", "600", "700", "800", "900", "950",
];

/// Utilities taking a color.
const COLORED: &[&str] = &[
    "bg",
    "text",
    "border",
    "border-t",
    "border-b",
    "border-l",
    "border-r",
    "ring",
    "ring-offset",
    "outline",
    "divide",
    "fill",
    "stroke",
    "from",
    "via",
    "to",
    "placeholder",
    "decoration",
    "accent",
    "caret",
    "shadow",
];

/// Utilities with a fixed set of values: (prefix, values); an empty
/// value stands for the bare prefix.
const SCALES: &[(&str, &[&str])] = &[
    (
        "text",
        &[
            "xs", "sm", "base", "lg", "xl", "2xl", "3xl", "4xl", "5xl", "6xl", "7xl", "8xl", "9xl",
            "left", "center", "right", "justify", "start", "end", "wrap", "nowrap", "balance",
            "pretty", "ellipsis", "clip",
        ],
    ),
    (
        "font",
        &[
            "thin",
            "extralight",
            "light",
            "normal",
            "medium",
            "semibold",
            "bold",
            "extrabold",
            "black",
            "sans",
            "serif",
            "mono",
        ],
    ),
    (
        "rounded",
        &[
            "", "none", "sm", "md", "lg", "xl", "2xl", "3xl", "full", "t-lg", "b-lg", "l-lg",
            "r-lg",
        ],
    ),
    (
        "shadow",
        &["", "sm", "md", "lg", "xl", "2xl", "inner", "none"],
    ),
    (
        "border",
        &[
            "", "0", "2", "4", "8", "t", "b", "l", "r", "x", "y", "solid", "dashed", "dotted",
            "none",
        ],
    ),
    ("ring", &["", "0", "1", "2", "4", "8", "inset"]),
    (
        "opacity",
        &[
            "0", "5", "10", "20", "25", "30", "40", "50", "60", "70", "75", "80", "90", "95", "100",
        ],
    ),
    ("z", &["0", "10", "20", "30", "40", "50", "auto"]),
    (
        "leading",
        &[
            "none", "tight", "snug", "normal", "relaxed", "loose", "3", "4", "5", "6", "7", "8",
            "9", "10",
        ],
    ),
    (
        "tracking",
        &["tighter", "tight", "normal", "wide", "wider", "widest"],
    ),
    (
        "max-w",
        &[
            "none",
            "xs",
            "sm",
            "md",
            "lg",
            "xl",
            "2xl",
            "3xl",
            "4xl",
            "5xl",
            "6xl",
            "7xl",
            "full",
            "prose",
            "screen-sm",
            "screen-md",
            "screen-lg",
            "screen-xl",
        ],
    ),
    (
        "grid-cols",
        &[
            "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "none", "subgrid",
        ],
    ),
    (
        "grid-rows",
        &["1", "2", "3", "4", "5", "6", "none", "subgrid"],
    ),
    (
        "col-span",
        &[
            "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "full",
        ],
    ),
    ("row-span", &["1", "2", "3", "4", "5", "6", "full"]),
    (
        "flex",
        &[
            "",
            "1",
            "auto",
            "initial",
            "none",
            "row",
            "row-reverse",
            "col",
            "col-reverse",
            "wrap",
            "wrap-reverse",
            "nowrap",
        ],
    ),
    ("items", &["start", "end", "center", "baseline", "stretch"]),
    (
        "justify",
        &[
            "start", "end", "center", "between", "around", "evenly", "stretch",
        ],
    ),
    (
        "content",
        &[
            "start", "end", "center", "between", "around", "evenly", "none",
        ],
    ),
    (
        "self",
        &["auto", "start", "end", "center", "stretch", "baseline"],
    ),
    ("place", &["items-center", "content-center", "self-center"]),
    (
        "overflow",
        &[
            "auto", "hidden", "clip", "visible", "scroll", "x-auto", "y-auto", "x-hidden",
            "y-hidden",
        ],
    ),
    (
        "object",
        &[
            "contain",
            "cover",
            "fill",
            "none",
            "scale-down",
            "center",
            "top",
            "bottom",
        ],
    ),
    (
        "cursor",
        &[
            "auto",
            "default",
            "pointer",
            "wait",
            "text",
            "move",
            "not-allowed",
            "grab",
        ],
    ),
    (
        "transition",
        &[
            "",
            "none",
            "all",
            "colors",
            "opacity",
            "shadow",
            "transform",
        ],
    ),
    (
        "duration",
        &["75", "100", "150", "200", "300", "500", "700", "1000"],
    ),
    ("ease", &["linear", "in", "out", "in-out"]),
    ("animate", &["none", "spin", "ping", "pulse", "bounce"]),
    (
        "scale",
        &[
            "0", "50", "75", "90", "95", "100", "105", "110", "125", "150",
        ],
    ),
    (
        "rotate",
        &["0", "1", "2", "3", "6", "12", "45", "90", "180"],
    ),
    ("blur", &["", "none", "sm", "md", "lg", "xl", "2xl", "3xl"]),
    ("aspect", &["auto", "square", "video"]),
    (
        "order",
        &["first", "last", "none", "1", "2", "3", "4", "5", "6"],
    ),
    (
        "whitespace",
        &[
            "normal",
            "nowrap",
            "pre",
            "pre-line",
            "pre-wrap",
            "break-spaces",
        ],
    ),
    ("break", &["normal", "words", "all", "keep"]),
    ("list", &["none", "disc", "decimal", "inside", "outside"]),
    ("select", &["none", "text", "all", "auto"]),
    ("pointer-events", &["none", "auto"]),
    (
        "bg",
        &[
            "transparent",
            "current",
            "black",
            "white",
            "cover",
            "contain",
            "center",
            "no-repeat",
            "fixed",
            "gradient-to-r",
            "gradient-to-l",
            "gradient-to-t",
            "gradient-to-b",
            "gradient-to-br",
            "gradient-to-tr",
        ],
    ),
    ("text", &["transparent", "current", "black", "white"]),
    ("border", &["transparent", "current", "black", "white"]),
    (
        "decoration",
        &[
            "solid", "double", "dotted", "dashed", "wavy", "0", "1", "2", "4", "8",
        ],
    ),
    ("underline", &["offset-1", "offset-2", "offset-4"]),
    (
        "line",
        &[
            "clamp-1",
            "clamp-2",
            "clamp-3",
            "clamp-4",
            "clamp-5",
            "clamp-6",
            "clamp-none",
        ],
    ),
];

/// Classes without a value.
const STANDALONE: &[&str] = &[
    "block",
    "inline-block",
    "inline",
    "flex",
    "inline-flex",
    "grid",
    "inline-grid",
    "table",
    "contents",
    "hidden",
    "flow-root",
    "static",
    "fixed",
    "absolute",
    "relative",
    "sticky",
    "visible",
    "invisible",
    "collapse",
    "container",
    "truncate",
    "underline",
    "overline",
    "line-through",
    "no-underline",
    "uppercase",
    "lowercase",
    "capitalize",
    "normal-case",
    "italic",
    "not-italic",
    "antialiased",
    "subpixel-antialiased",
    "grow",
    "grow-0",
    "shrink",
    "shrink-0",
    "sr-only",
    "not-sr-only",
    "isolate",
    "transform",
    "filter",
    "outline-none",
    "resize",
    "resize-none",
    "appearance-none",
    "prose",
    "group",
    "peer",
    "box-border",
    "box-content",
    "float-left",
    "float-right",
    "float-none",
    "clear-both",
    "mx-auto",
    "my-auto",
    "m-auto",
    "ml-auto",
    "mr-auto",
];

/// Every class of the default theme, built once.
fn classes() -> &'static [(String, &'static str)] {
    static ALL: OnceLock<Vec<(String, &'static str)>> = OnceLock::new();
    ALL.get_or_init(|| {
        let mut out: Vec<(String, &'static str)> = Vec::new();
        out.extend(STANDALONE.iter().map(|c| ((*c).to_string(), "utility")));
        for prefix in SPACED {
            for v in SPACING {
                out.push((format!("{prefix}-{v}"), "spacing"));
            }
            if prefix.starts_with('m')
                || prefix.starts_with("inset")
                || prefix.starts_with("translate")
            {
                for v in SPACING.iter().skip(1) {
                    out.push((format!("-{prefix}-{v}"), "spacing"));
                }
            }
            if matches!(*prefix, "w" | "h" | "size" | "min-w" | "min-h" | "max-h") {
                for v in SIZES {
                    out.push((format!("{prefix}-{v}"), "size"));
                }
            }
        }
        for prefix in COLORED {
            for color in COLORS {
                for shade in SHADES {
                    out.push((format!("{prefix}-{color}-{shade}"), "color"));
                }
            }
        }
        for (prefix, values) in SCALES {
            for v in *values {
                out.push((
                    if v.is_empty() {
                        (*prefix).to_string()
                    } else {
                        format!("{prefix}-{v}")
                    },
                    "utility",
                ));
            }
        }
        out.sort();
        out.dedup_by(|a, b| a.0 == b.0);
        out
    })
}

/// Whether `before` (text up to the caret) ends inside the value of a
/// `class` or `className` attribute (also Vue's `:class` with a string,
/// Twig and Blade echo aside); returns where the value starts.
fn class_value_start(before: &str) -> Option<usize> {
    let tag_open = before.rfind('<')?;
    let tag = &before[tag_open..];
    if tag.contains('>') {
        return None;
    }
    for quote in ['"', '\''] {
        let Some(open) = tag.rfind(quote) else {
            continue;
        };
        if tag.matches(quote).count() % 2 == 0 {
            continue;
        }
        let Some(name) = tag[..open].trim_end().strip_suffix('=') else {
            continue;
        };
        let name = name.trim_end();
        let name = name.rsplit(char::is_whitespace).next().unwrap_or(name);
        if matches!(
            name,
            "class" | "className" | ":class" | "x-bind:class" | "classList"
        ) {
            return Some(tag_open + open + 1);
        }
    }
    None
}

/// Whether `offset` sits inside a class attribute value, where Tailwind
/// completion applies; cheap enough to ask on every keystroke.
pub fn in_class_attribute(src: &str, offset: usize) -> bool {
    src.get(..offset).and_then(class_value_start).is_some()
}

/// Tailwind proposals at `offset` in a template: the class being typed,
/// keeping any `variant:` prefixes; `None` outside class attributes.
pub fn tailwind_completions(src: &str, offset: usize) -> Option<(usize, Vec<Candidate>)> {
    let before = src.get(..offset)?;
    let value_start = class_value_start(before)?;
    let token_start = before[value_start..]
        .rfind(|c: char| c.is_whitespace() || c == '\'' || c == '"')
        .map_or(value_start, |i| value_start + i + 1);
    let token = &before[token_start..];
    let (variants, prefix) = match token.rfind(':') {
        Some(i) => (&token[..=i], &token[i + 1..]),
        None => ("", token),
    };
    let negative = prefix.strip_prefix('-').is_some();
    let mut items: Vec<Candidate> = Vec::new();
    if variants.is_empty() {
        items.extend(
            VARIANTS
                .iter()
                .filter(|v| v.starts_with(prefix) && !prefix.is_empty())
                .map(|v| Candidate {
                    label: format!("{v}:"),
                    kind: Kind::Constant,
                    detail: "variant".into(),
                    insert_text: format!("{v}:"),
                    import: None,
                }),
        );
    }
    items.extend(
        classes()
            .iter()
            .filter(|(c, _)| c.starts_with(prefix) && (negative || !c.starts_with('-')))
            .take(LIMIT)
            .map(|(class, detail)| Candidate {
                label: format!("{variants}{class}"),
                kind: Kind::Property,
                detail: (*detail).to_string(),
                insert_text: format!("{variants}{class}"),
                import: None,
            }),
    );
    Some((token_start, items))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Labels proposed at the end of `src`.
    fn labels(src: &str) -> Vec<String> {
        tailwind_completions(src, src.len())
            .map(|(_, items)| items.into_iter().map(|c| c.label).collect())
            .unwrap_or_default()
    }

    #[test]
    fn class_attributes_should_propose_utilities() {
        let found = labels("<div class=\"flex bg-red-");
        assert!(found.contains(&"bg-red-500".to_string()));
    }

    #[test]
    fn variants_should_be_kept_in_proposals() {
        let found = labels("<a class=\"md:hover:px-");
        assert!(found.contains(&"md:hover:px-4".to_string()));
    }

    #[test]
    fn other_attributes_should_get_nothing() {
        assert!(tailwind_completions("<a href=\"px-", 12).is_none());
    }

    #[test]
    fn replaced_text_should_start_at_the_class() {
        let src = "<div class=\"flex  items-c";
        let (start, _) = tailwind_completions(src, src.len()).unwrap();
        assert_eq!(&src[start..], "items-c");
    }
}
