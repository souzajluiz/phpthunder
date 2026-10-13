//! Built-in `.env` formatting: `KEY = value` becomes `KEY=value`, since
//! dotenv parsers differ on spaces around `=`. Values and comments are left
//! as written.

/// Removes the spaces around the `=` of every assignment line.
pub fn format(text: &str) -> String {
    text.split('\n')
        .map(|line| {
            let body = line.trim_start();
            let (export, body) = match body.strip_prefix("export ") {
                Some(rest) => ("export ", rest.trim_start()),
                None => ("", body),
            };
            let Some((key, value)) = body.split_once('=') else {
                return line.to_string();
            };
            let key = key.trim_end();
            let is_key = !key.is_empty()
                && !key.starts_with('#')
                && key
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
            if is_key {
                format!("{export}{key}={}", value.trim_start())
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_should_tighten_assignments_only() {
        let src = "# App = name\nAPP_NAME = \"My App\"\n  export DB_HOST =  127.0.0.1\nURL=a=b\n";
        assert_eq!(
            format(src),
            "# App = name\nAPP_NAME=\"My App\"\nexport DB_HOST=127.0.0.1\nURL=a=b\n"
        );
    }
}
