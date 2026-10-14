//! HTTP client for `.http` / `.rest` files: requests separated by `###`
//! lines, each a request line (`GET https://…`), headers, a blank line and
//! an optional body; `{{name}}` variables from `@name = value` lines and
//! from the `http-client.env.json` environment next to the file (or in the
//! project). Requests are sent with the system's `curl`.

use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

/// Methods a request line may start with.
const METHODS: &[&str] = &[
    "GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "TRACE", "CONNECT",
];

/// One request of a `.http` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// The `### name` above it, if any.
    pub name: String,
    /// HTTP method.
    pub method: String,
    /// Target URL, variables not yet replaced.
    pub url: String,
    /// Header lines as (name, value).
    pub headers: Vec<(String, String)>,
    /// Body text, empty when none.
    pub body: String,
    /// Byte range of the request in the file.
    pub range: std::ops::Range<usize>,
}

/// `@name = value` variables declared in `src`.
pub fn file_variables(src: &str) -> HashMap<String, String> {
    src.lines()
        .filter_map(|l| l.trim().strip_prefix('@'))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

/// Parses every request of `src`.
pub fn requests(src: &str) -> Vec<Request> {
    let mut out = Vec::new();
    let mut offset = 0;
    // Blocks between `###` separators.
    let mut blocks: Vec<(usize, String, Vec<&str>)> = vec![(0, String::new(), Vec::new())];
    for line in src.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        let text = line.trim_end_matches(['\n', '\r']);
        if let Some(name) = text.strip_prefix("###") {
            blocks.push((start, name.trim().to_string(), Vec::new()));
        } else if let Some(block) = blocks.last_mut() {
            block.2.push(line);
        }
    }
    let ends: Vec<usize> = blocks
        .iter()
        .skip(1)
        .map(|b| b.0)
        .chain([src.len()])
        .collect();
    for ((start, name, lines), end) in blocks.into_iter().zip(ends) {
        let mut rest = lines
            .iter()
            .map(|l| l.trim_end_matches(['\n', '\r']))
            .skip_while(|l| {
                let t = l.trim();
                t.is_empty() || t.starts_with('#') || t.starts_with("//") || t.starts_with('@')
            });
        let Some(request_line) = rest.next() else {
            continue;
        };
        let mut parts = request_line.split_whitespace();
        let first = parts.next().unwrap_or_default();
        let (method, url) = if METHODS.contains(&first.to_ascii_uppercase().as_str()) {
            (
                first.to_ascii_uppercase(),
                parts.next().unwrap_or_default().to_string(),
            )
        } else if first.contains("://") || first.starts_with('{') || first.starts_with('/') {
            ("GET".to_string(), first.to_string())
        } else {
            continue;
        };
        let mut headers = Vec::new();
        let mut body = Vec::new();
        let mut in_body = false;
        for line in rest {
            if in_body {
                body.push(line);
            } else if line.trim().is_empty() {
                in_body = true;
            } else if let Some((k, v)) = line.split_once(':')
                && !line.trim_start().starts_with('#')
            {
                headers.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
        let body = body
            .iter()
            .filter(|l| !l.trim_start().starts_with("> "))
            .copied()
            .collect::<Vec<_>>()
            .join("\n")
            .trim_end()
            .to_string();
        out.push(Request {
            name,
            method,
            url,
            headers,
            body,
            range: start..end,
        });
    }
    out
}

/// The request whose block holds `offset`.
pub fn request_at(src: &str, offset: usize) -> Option<Request> {
    let all = requests(src);
    all.iter()
        .find(|r| r.range.contains(&offset))
        .or_else(|| all.last().filter(|r| offset >= r.range.end))
        .cloned()
}

/// Variables of environment `env` from the `http-client.env.json` (and the
/// private `http-client.private.env.json`) in `dir`, then in `root`.
pub fn environment(dir: &Path, root: &Path, env: &str) -> HashMap<String, String> {
    let mut vars = HashMap::new();
    for folder in [root, dir] {
        for file in ["http-client.env.json", "http-client.private.env.json"] {
            let Some(json) = std::fs::read_to_string(folder.join(file))
                .ok()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            else {
                continue;
            };
            for (k, v) in json[env].as_object().into_iter().flatten() {
                let value = match v {
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                vars.insert(k.clone(), value);
            }
        }
    }
    vars
}

/// `text` with `{{name}}` replaced from `vars` (unknown names kept).
pub fn substitute(text: &str, vars: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find("{{") {
        out.push_str(&rest[..open]);
        let after = &rest[open + 2..];
        let Some(close) = after.find("}}") else {
            out.push_str(&rest[open..]);
            return out;
        };
        let name = after[..close].trim();
        match vars.get(name) {
            Some(v) => out.push_str(v),
            None => out.push_str(&rest[open..open + 4 + close]),
        }
        rest = &after[close + 2..];
    }
    out.push_str(rest);
    out
}

/// `curl` arguments for `request` with variables filled in.
pub fn curl_args(request: &Request, vars: &HashMap<String, String>) -> Vec<String> {
    let mut args = vec![
        "-sS".to_string(),
        "-i".to_string(),
        "-X".to_string(),
        request.method.clone(),
    ];
    for (k, v) in &request.headers {
        args.push("-H".into());
        args.push(format!("{}: {}", substitute(k, vars), substitute(v, vars)));
    }
    if !request.body.is_empty() {
        args.push("--data-binary".into());
        args.push("@-".into());
    }
    args.push("--".into());
    args.push(substitute(&request.url, vars));
    args
}

/// Sends `request` with `curl` and returns the response as text: status
/// line, headers, a blank line, then the body (JSON indented), timed.
///
/// # Errors
/// Returns `curl`'s error (missing tool, connection refused, bad URL).
pub async fn send(request: Request, vars: HashMap<String, String>) -> Result<String, String> {
    use tokio::io::AsyncWriteExt;
    let started = std::time::Instant::now();
    let mut child = tokio::process::Command::new("curl")
        .args(curl_args(&request, &vars))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("curl: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let body = substitute(&request.body, &vars);
        stdin
            .write_all(body.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
    }
    let out = child.wait_with_output().await.map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let text = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    // `-i` may print several header blocks (redirects, 100 Continue): the
    // body follows the last one.
    let (head, body) = split_response(&text);
    let pretty = serde_json::from_str::<Value>(body.trim())
        .ok()
        .and_then(|v| serde_json::to_string_pretty(&v).ok())
        .unwrap_or_else(|| body.to_string());
    Ok(format!(
        "{} {}\n# {} ms\n\n{}\n\n{}\n",
        request.method,
        substitute(&request.url, &vars),
        started.elapsed().as_millis(),
        head.trim_end(),
        pretty.trim_end()
    ))
}

/// Splits `curl -i` output into the last header block and the body.
fn split_response(text: &str) -> (&str, &str) {
    let mut start = 0;
    loop {
        let Some(gap) = text[start..].find("\n\n") else {
            return (text, "");
        };
        let head_end = start + gap;
        let body_start = head_end + 2;
        if text[body_start..].starts_with("HTTP/") {
            start = body_start;
            continue;
        }
        return (&text[start..head_end], &text[body_start..]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = "@host = http://localhost:8000\n\n### List users\nGET {{host}}/users\nAccept: application/json\n\n### Create\nPOST {{host}}/users\nContent-Type: application/json\n\n{\"name\": \"{{name}}\"}\n";

    #[test]
    fn requests_should_split_on_separators() {
        let all = requests(SRC);
        assert_eq!(
            all.iter()
                .map(|r| (r.name.as_str(), r.method.as_str()))
                .collect::<Vec<_>>(),
            [("List users", "GET"), ("Create", "POST")]
        );
        assert_eq!(all[1].body, "{\"name\": \"{{name}}\"}");
        assert_eq!(
            all[0].headers,
            [("Accept".to_string(), "application/json".to_string())]
        );
    }

    #[test]
    fn request_at_should_pick_the_block_of_the_caret() {
        let at = SRC.find("Content-Type").unwrap();
        assert_eq!(request_at(SRC, at).unwrap().name, "Create");
    }

    #[test]
    fn variables_should_fill_urls_and_keep_unknown_names() {
        let mut vars = file_variables(SRC);
        vars.insert("name".into(), "Ana".into());
        let r = &requests(SRC)[1];
        assert_eq!(substitute(&r.url, &vars), "http://localhost:8000/users");
        assert_eq!(substitute("{{x}}", &vars), "{{x}}");
        assert_eq!(
            curl_args(r, &vars).last().unwrap(),
            "http://localhost:8000/users"
        );
    }

    #[test]
    fn redirect_headers_should_be_skipped() {
        let text = "HTTP/1.1 301 Moved\nLocation: /b\n\nHTTP/1.1 200 OK\nX: 1\n\n{}";
        assert_eq!(split_response(text), ("HTTP/1.1 200 OK\nX: 1", "{}"));
    }
}
