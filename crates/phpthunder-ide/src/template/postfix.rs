//! Postfix templates: an expression followed by `.key` and Tab turns into
//! code around that expression (`$user.if` → `if ($user) { }`,
//! `$items.foreach` → a `foreach` loop, `$x.return` → `return $x;`).

use super::*;

/// One postfix template: the key after the dot and the code it becomes,
/// where `$EXPR$` stands for the expression and [`END`] for the caret.
pub struct Postfix {
    /// Word typed after the dot.
    pub key: &'static str,
    /// Replacement; `$EXPR$` is the expression, [`END`] the caret.
    pub body: &'static str,
}

/// Placeholder for the expression in a postfix body.
const EXPR: &str = "$EXPR$";

/// Every postfix template, by key.
pub const POSTFIX: &[Postfix] = &[
    Postfix {
        key: "if",
        body: "if ($EXPR$) {\n    $END$\n}",
    },
    Postfix {
        key: "else",
        body: "if (!$EXPR$) {\n    $END$\n}",
    },
    Postfix {
        key: "notnull",
        body: "if ($EXPR$ !== null) {\n    $END$\n}",
    },
    Postfix {
        key: "nn",
        body: "if ($EXPR$ !== null) {\n    $END$\n}",
    },
    Postfix {
        key: "null",
        body: "if ($EXPR$ === null) {\n    $END$\n}",
    },
    Postfix {
        key: "while",
        body: "while ($EXPR$) {\n    $END$\n}",
    },
    Postfix {
        key: "foreach",
        body: "foreach ($EXPR$ as $value) {\n    $END$\n}",
    },
    Postfix {
        key: "fore",
        body: "foreach ($EXPR$ as $value) {\n    $END$\n}",
    },
    Postfix {
        key: "forek",
        body: "foreach ($EXPR$ as $key => $value) {\n    $END$\n}",
    },
    Postfix {
        key: "not",
        body: "!$EXPR$$END$",
    },
    Postfix {
        key: "isset",
        body: "isset($EXPR$)$END$",
    },
    Postfix {
        key: "empty",
        body: "empty($EXPR$)$END$",
    },
    Postfix {
        key: "count",
        body: "count($EXPR$)$END$",
    },
    Postfix {
        key: "par",
        body: "($EXPR$)$END$",
    },
    Postfix {
        key: "return",
        body: "return $EXPR$;$END$",
    },
    Postfix {
        key: "echo",
        body: "echo $EXPR$;$END$",
    },
    Postfix {
        key: "throw",
        body: "throw $EXPR$;$END$",
    },
    Postfix {
        key: "var",
        body: "$$END$ = $EXPR$;",
    },
    Postfix {
        key: "dd",
        body: "dd($EXPR$);$END$",
    },
    Postfix {
        key: "new",
        body: "new $EXPR$($END$)",
    },
];

/// Start of the expression that ends just before byte `end`: names,
/// variables, `->`/`?->`/`::` chains and balanced brackets, walking left.
fn expression_start(text: &str, end: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut i = end;
    while i > 0 {
        let c = bytes[i - 1];
        if c.is_ascii_alphanumeric() || c == b'_' || c == b'$' || c == b'\\' || c >= 0x80 {
            i -= 1;
        } else if c == b'>' && i >= 2 && bytes[i - 2] == b'-' {
            i -= 2;
            if i > 0 && bytes[i - 1] == b'?' {
                i -= 1;
            }
        } else if c == b':' && i >= 2 && bytes[i - 2] == b':' {
            i -= 2;
        } else if matches!(c, b')' | b']') {
            let open = if c == b')' { b'(' } else { b'[' };
            let mut depth = 0usize;
            let mut j = i;
            loop {
                if j == 0 {
                    return None;
                }
                j -= 1;
                if bytes[j] == c {
                    depth += 1;
                } else if bytes[j] == open {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
            }
            i = j;
        } else {
            break;
        }
    }
    (i < end).then_some(i)
}

/// The postfix expansion for `expr.key` just before `offset`, if `key`
/// names a postfix template.
pub fn expand_postfix(text: &str, offset: usize) -> Option<Expansion> {
    let before = text.get(..offset)?;
    let key_start = before
        .rfind(|c: char| !c.is_ascii_alphanumeric())
        .map_or(0, |i| i + 1);
    let key = &before[key_start..];
    let postfix = POSTFIX.iter().find(|p| p.key == key)?;
    let dot = key_start.checked_sub(1)?;
    if before.as_bytes().get(dot) != Some(&b'.') {
        return None;
    }
    let start = expression_start(text, dot)?;
    let expr = &text[start..dot];
    let body = postfix.body.replace(EXPR, expr);
    Some(render_body(&body, text, start, offset))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_postfix_should_wrap_a_variable_in_if() {
        let text = "<?php\n    $user.if";
        let e = expand_postfix(text, text.len()).unwrap();
        assert_eq!(e.text, "if ($user) {\n        \n    }");
    }

    #[test]
    fn expand_postfix_should_take_a_whole_call_chain() {
        let text = "<?php $this->items()->first().return";
        let e = expand_postfix(text, text.len()).unwrap();
        assert_eq!(e.text, "return $this->items()->first();");
    }

    #[test]
    fn expand_postfix_should_ignore_unknown_keys() {
        assert_eq!(expand_postfix("$a.nope", 7), None);
    }

    #[test]
    fn expand_postfix_should_place_the_caret_in_var() {
        let text = "<?php f(1).var";
        let e = expand_postfix(text, text.len()).unwrap();
        assert_eq!((e.text.as_str(), e.caret), ("$ = f(1);", 7));
    }
}
