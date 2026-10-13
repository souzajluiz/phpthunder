//! PHPDoc parsing: tags that carry type information, plus the summary line.

use super::*;
use crate::syntax::{NameContext, is_builtin_type};

/// A PHPDoc tag that carries type information; [`parse`] drops all others.
///
/// Types are kept as written; [`resolve`] turns them into FQNs later.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tag {
    /// `@return Type` (also `@phpstan-return` and `@psalm-return`).
    Return {
        /// Type expression as written.
        ty: String,
    },
    /// `@var Type [$name]`, including the legacy `@var $name Type` order.
    Var {
        /// Type expression as written.
        ty: String,
        /// Variable name without `$`, if given.
        var: Option<String>,
    },
    /// `@param Type $name` (also variadic `...$name`).
    Param {
        /// Type expression as written.
        ty: String,
        /// Parameter name without `$`.
        var: String,
    },
    /// `@property`, `@property-read`, or `@property-write`: a virtual property.
    Property {
        /// Type expression, if given.
        ty: Option<String>,
        /// Property name without `$`.
        name: String,
    },
    /// `@method [static] [Type] name(...)`: a virtual method.
    Method {
        /// Declared with `static`.
        is_static: bool,
        /// Return type expression, if given.
        ty: Option<String>,
        /// Method name.
        name: String,
    },
    /// `@mixin Class`: members of `class` are visible on the documented class.
    Mixin {
        /// Class name as written.
        class: String,
    },
}

/// A tag plus its byte offset inside the comment (for go to definition on
/// virtual members) and its source line (for hover).
#[derive(Clone, Debug)]
pub struct TagAt {
    /// The parsed tag.
    pub tag: Tag,
    /// Byte offset of the `@` from the start of the comment.
    pub offset: usize,
    /// The tag's line with comment decoration (`*`, `*/`) stripped.
    pub line: String,
}

/// A parsed `/** ... */` comment: summary plus the type-carrying tags.
#[derive(Clone, Debug, Default)]
pub struct DocBlock {
    /// First paragraph of the description, lines joined with spaces; shown on hover.
    pub summary: String,
    /// Recognized tags in source order.
    pub tags: Vec<TagAt>,
}

impl DocBlock {
    /// Type expression of the first `@return`, unresolved.
    pub fn return_type(&self) -> Option<&str> {
        self.tags.iter().find_map(|t| match &t.tag {
            Tag::Return { ty } => Some(ty.as_str()),
            _ => None,
        })
    }

    /// `@var Type` or `@var Type $name` (matching `name`, without `$`).
    pub fn var_type(&self, name: Option<&str>) -> Option<&str> {
        self.tags.iter().find_map(|t| match &t.tag {
            Tag::Var { ty, var } if var.is_none() || name.is_none() || var.as_deref() == name => {
                Some(ty.as_str())
            }
            _ => None,
        })
    }

    /// Type expression of the `@param` for `name` (without `$`), unresolved.
    pub fn param_type(&self, name: &str) -> Option<&str> {
        self.tags.iter().find_map(|t| match &t.tag {
            Tag::Param { ty, var } if var == name => Some(ty.as_str()),
            _ => None,
        })
    }
}

/// True for `/** ... */` docblocks; `/**/` is an ordinary empty comment. Only docblocks are worth [`parse`]-ing.
pub fn is_doc_comment(text: &str) -> bool {
    text.starts_with("/**") && text != "/**/"
}

/// Splits a type expression at the first top-level whitespace:
/// `array<int, User> $users rest` -> (`array<int, User>`, `$users rest`).
fn split_type(s: &str) -> (&str, &str) {
    let mut depth = 0i32;
    for (i, ch) in s.char_indices() {
        match ch {
            '<' | '(' | '{' | '[' => depth += 1,
            '>' | ')' | '}' | ']' => depth -= 1,
            c if c.is_whitespace() && depth <= 0 => return (&s[..i], s[i..].trim_start()),
            _ => {}
        }
    }
    (s, "")
}

/// First whitespace-separated word of `s`, or empty.
fn first_word(s: &str) -> &str {
    s.split_whitespace().next().unwrap_or("")
}

/// Parses the body of one `@name` tag; `None` for unrecognized or malformed tags (e.g. `@param` without a variable).
fn parse_tag(name: &str, body: &str) -> Option<Tag> {
    match name {
        "return" | "phpstan-return" | "psalm-return" => {
            let (ty, _) = split_type(body);
            (!ty.is_empty()).then(|| Tag::Return { ty: ty.into() })
        }
        "var" | "phpstan-var" | "psalm-var" => {
            let (ty, rest) = split_type(body);
            if let Some(name) = ty.strip_prefix('$') {
                // `@var $name Type` (legacy order)
                let (real, _) = split_type(rest);
                return Some(Tag::Var {
                    ty: real.into(),
                    var: Some(name.into()),
                });
            }
            let var = first_word(rest).strip_prefix('$').map(str::to_string);
            (!ty.is_empty()).then(|| Tag::Var { ty: ty.into(), var })
        }
        "param" | "phpstan-param" | "psalm-param" => {
            let (ty, rest) = split_type(body);
            let var = first_word(rest)
                .trim_start_matches("...")
                .strip_prefix('$')?;
            Some(Tag::Param {
                ty: ty.into(),
                var: var.into(),
            })
        }
        "property" | "property-read" | "property-write" => {
            let (first, rest) = split_type(body);
            if let Some(name) = first.strip_prefix('$') {
                return Some(Tag::Property {
                    ty: None,
                    name: name.into(),
                });
            }
            let name = first_word(rest).strip_prefix('$')?;
            Some(Tag::Property {
                ty: Some(first.into()),
                name: name.into(),
            })
        }
        "method" => {
            let head = &body[..body.find('(')?];
            let words: Vec<&str> = head.split_whitespace().collect();
            let (&name, rest) = words.split_last()?;
            let (is_static, ty) = match rest {
                ["static", ty] => (true, Some(*ty)),
                [ty] => (false, Some(*ty)),
                _ => (false, None),
            };
            Some(Tag::Method {
                is_static,
                ty: ty.map(str::to_string),
                name: name.into(),
            })
        }
        "mixin" => {
            let class = first_word(body);
            (!class.is_empty()).then(|| Tag::Mixin {
                class: class.into(),
            })
        }
        _ => None,
    }
}

/// Parses a raw `/** ... */` comment.
///
/// Never fails: unknown tags and malformed lines are skipped. Called for
/// declaration docblocks while building stubs and for inline `@var` comments
/// during type inference.
pub fn parse_docblock(comment: &str) -> DocBlock {
    let mut doc = DocBlock::default();
    let mut summary: Vec<&str> = Vec::new();
    let mut in_summary = true;
    let mut offset = 0;
    for raw_line in comment.split_inclusive('\n') {
        let line_offset = offset;
        offset += raw_line.len();
        let line = raw_line
            .trim()
            .trim_start_matches("/**")
            .trim_end_matches("*/")
            .trim_start_matches('*')
            .trim();
        if let Some(tag_text) = line.strip_prefix('@') {
            in_summary = false;
            let (name, body) = match tag_text.split_once(char::is_whitespace) {
                Some((n, b)) => (n, b.trim()),
                None => (tag_text, ""),
            };
            if let Some(tag) = parse_tag(name, body) {
                let at = raw_line.find('@').unwrap_or(0);
                doc.tags.push(TagAt {
                    tag,
                    offset: line_offset + at,
                    line: line.to_string(),
                });
            }
        } else if in_summary {
            if line.is_empty() {
                in_summary = summary.is_empty();
            } else {
                summary.push(line);
            }
        }
    }
    doc.summary = summary.join(" ");
    doc
}

/// Splits `A|B` at top level, ignoring `|` inside generics.
fn union_parts(ty: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0i32, 0);
    for (i, ch) in ty.char_indices() {
        match ch {
            '<' | '(' | '{' => depth += 1,
            '>' | ')' | '}' => depth -= 1,
            '|' if depth == 0 => {
                parts.push(&ty[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&ty[start..]);
    parts
}

/// Resolves one class name from a PHPDoc type to an FQN relative to `ctx`.
///
/// `self` becomes `owner` and `static`/`$this` become [`STATIC_TYPE`]; builtins,
/// pseudo-types (`array-key`, `class-string`, ...), literals, and variables
/// yield `None`.
fn resolve_name(name: &str, ctx: &NameContext, owner: Option<&str>) -> Option<String> {
    let name = name.trim();
    match name.to_ascii_lowercase().as_str() {
        "" | "null" | "resource" | "scalar" | "numeric" | "array-key" | "class-string"
        | "positive-int" | "negative-int" | "non-empty-string" | "list" | "non-empty-array"
        | "non-empty-list" | "key-of" | "value-of" => None,
        "self" => owner.map(str::to_string),
        "static" | "$this" => Some(STATIC_TYPE.to_string()),
        _ if is_builtin_type(name) => None,
        _ if name.starts_with('$') || name.starts_with('\'') || name.starts_with('"') => None,
        _ if name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit() || c == '-') =>
        {
            None
        }
        _ => Some(ctx.resolve_class(name)),
    }
}

/// Resolves a PHPDoc type expression. The first class-like union member wins.
///
/// Understands `?T`, `T[]`, and `Generic<..., T>` (the last generic argument
/// becomes the element type). `owner` is the class that `self` refers to.
pub fn resolve_doc_type(ty: &str, ctx: &NameContext, owner: Option<&str>) -> Ty {
    let mut out = Ty::default();
    for part in union_parts(ty.trim().trim_start_matches('?')) {
        let part = part
            .trim()
            .trim_start_matches('?')
            .trim_matches(|c| c == '(' || c == ')');
        let t = if let Some(elem) = part.strip_suffix("[]") {
            Ty {
                class: None,
                elem: resolve_doc_type(elem, ctx, owner).class,
            }
        } else if let Some(open) = part.find('<') {
            let base = &part[..open];
            let args = part[open + 1..].trim_end_matches('>');
            let last_arg = union_parts(args)
                .last()
                .and_then(|a| a.rsplit(',').next())
                .unwrap_or("");
            Ty {
                class: resolve_name(base, ctx, owner),
                elem: resolve_doc_type(last_arg, ctx, owner).class,
            }
        } else {
            Ty {
                class: resolve_name(part, ctx, owner),
                elem: None,
            }
        };
        out = out.or(t);
        if out.class.is_some() && out.elem.is_some() {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tags_and_summary() {
        let doc = parse_docblock(
            "/**\n * Finds users.\n * Second line.\n *\n * Details.\n * @param array<int, User> $ids the ids\n * @return Collection<int, User>|null\n * @var Foo $foo\n * @property-read Address $address\n * @property $loose\n * @method static Builder where(string $col)\n * @method save()\n * @mixin \\Eloquent\n */",
        );
        assert_eq!(doc.summary, "Finds users. Second line.");
        assert_eq!(doc.param_type("ids"), Some("array<int, User>"));
        assert_eq!(doc.return_type(), Some("Collection<int, User>|null"));
        assert_eq!(doc.var_type(Some("foo")), Some("Foo"));
        let tags: Vec<&Tag> = doc.tags.iter().map(|t| &t.tag).collect();
        assert!(tags.contains(&&Tag::Property {
            ty: Some("Address".into()),
            name: "address".into()
        }));
        assert!(tags.contains(&&Tag::Property {
            ty: None,
            name: "loose".into()
        }));
        assert!(tags.contains(&&Tag::Method {
            is_static: true,
            ty: Some("Builder".into()),
            name: "where".into()
        }));
        assert!(tags.contains(&&Tag::Method {
            is_static: false,
            ty: None,
            name: "save".into()
        }));
        assert!(tags.contains(&&Tag::Mixin {
            class: "\\Eloquent".into()
        }));
    }

    #[test]
    fn resolves_type_expressions() {
        let ctx = NameContext {
            namespace: "App".into(),
            ..Default::default()
        };
        assert_eq!(
            resolve_doc_type("?User", &ctx, None),
            Ty::class("App\\User")
        );
        assert_eq!(
            resolve_doc_type("null|User", &ctx, None),
            Ty::class("App\\User")
        );
        assert_eq!(
            resolve_doc_type("User[]", &ctx, None),
            Ty {
                class: None,
                elem: Some("App\\User".into())
            }
        );
        assert_eq!(
            resolve_doc_type("\\Illuminate\\Support\\Collection<int, User>", &ctx, None),
            Ty {
                class: Some("Illuminate\\Support\\Collection".into()),
                elem: Some("App\\User".into())
            }
        );
        assert_eq!(
            resolve_doc_type("array<string, User>", &ctx, None),
            Ty {
                class: None,
                elem: Some("App\\User".into())
            }
        );
        assert_eq!(
            resolve_doc_type("static", &ctx, None),
            Ty::class(STATIC_TYPE)
        );
        assert_eq!(
            resolve_doc_type("self", &ctx, Some("App\\Model")),
            Ty::class("App\\Model")
        );
        assert!(resolve_doc_type("int|string", &ctx, None).is_empty());
    }
}
