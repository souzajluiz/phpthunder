//! Xdebug's profiler and configuration check: running a script with
//! `xdebug.mode=profile`, reading the cachegrind file it writes into
//! per-function costs, and reporting how Xdebug is set up.

use crate::interpreter::Interpreter;
use crate::tr;
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;

/// Cost of one function over a profiled run.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FunctionCost {
    /// Function or method name as Xdebug writes it.
    pub name: String,
    /// File it is declared in (empty for internal functions).
    pub file: String,
    /// Time spent in the function itself, in milliseconds.
    pub self_ms: f64,
    /// Time including the functions it called, in milliseconds.
    pub inclusive_ms: f64,
    /// How many times it was called.
    pub calls: u64,
}

/// Resolves a cachegrind name: `(12) name` defines id 12, `(12)` refers to
/// it, anything else is literal.
fn resolve_name(table: &mut HashMap<String, String>, raw: &str) -> String {
    let raw = raw.trim();
    if let Some(rest) = raw.strip_prefix('(')
        && let Some((id, name)) = rest.split_once(')')
    {
        let name = name.trim();
        if name.is_empty() {
            return table.get(id).cloned().unwrap_or_default();
        }
        table.insert(id.to_string(), name.to_string());
        return name.to_string();
    }
    raw.to_string()
}

/// Reads a cachegrind file (Xdebug 3) into per-function costs, most
/// inclusive time first. The first event is time; Xdebug writes it in
/// units of 10 ns (`Time_(10ns)`), older versions in microseconds.
pub fn parse_cachegrind(text: &str) -> Vec<FunctionCost> {
    let mut files = HashMap::new();
    let mut names = HashMap::new();
    let mut costs: HashMap<String, FunctionCost> = HashMap::new();
    let mut unit_ms = 0.001;
    let (mut file, mut function) = (String::new(), String::new());
    let mut callee: Option<String> = None;
    let mut call_line_next = false;
    for line in text.lines() {
        if let Some(events) = line.strip_prefix("events:") {
            if events.contains("10ns") {
                unit_ms = 0.000_01;
            }
        } else if let Some(rest) = line.strip_prefix("fl=") {
            file = resolve_name(&mut files, rest);
        } else if let Some(rest) = line.strip_prefix("fn=") {
            function = resolve_name(&mut names, rest);
            let entry = costs.entry(function.clone()).or_default();
            entry.name.clone_from(&function);
            if entry.file.is_empty() {
                entry.file.clone_from(&file);
            }
        } else if line.starts_with("cfl=") || line.starts_with("cfi=") {
            let _ = resolve_name(&mut files, &line[4..]);
        } else if let Some(rest) = line.strip_prefix("cfn=") {
            callee = Some(resolve_name(&mut names, rest));
        } else if let Some(rest) = line.strip_prefix("calls=") {
            let count: u64 = rest
                .split_whitespace()
                .next()
                .and_then(|c| c.parse().ok())
                .unwrap_or(1);
            if let Some(c) = &callee {
                costs.entry(c.clone()).or_default().calls += count;
            }
            call_line_next = true;
        } else if line.starts_with(|c: char| c.is_ascii_digit() || c == '+' || c == '-') {
            let time: f64 = line
                .split_whitespace()
                .nth(1)
                .and_then(|t| t.parse().ok())
                .unwrap_or(0.0);
            let entry = costs.entry(function.clone()).or_default();
            if call_line_next {
                entry.inclusive_ms += time * unit_ms;
                call_line_next = false;
            } else {
                entry.self_ms += time * unit_ms;
                entry.inclusive_ms += time * unit_ms;
            }
        }
    }
    let mut out: Vec<FunctionCost> = costs
        .into_values()
        .filter(|c| !c.name.is_empty())
        .map(|mut c| {
            c.calls = c.calls.max(1);
            c
        })
        .collect();
    out.sort_by(|a, b| b.inclusive_ms.total_cmp(&a.inclusive_ms));
    out
}

/// A Markdown report of the costliest functions of a profiled run.
pub fn report(script: &str, costs: &[FunctionCost]) -> String {
    let mut out = format!("# {}\n\n", tr!("profile-title", script = script));
    out.push_str(&format!(
        "| {} | {} | {} | {} |\n|---|---:|---:|---:|\n",
        tr!("profile-function"),
        tr!("profile-inclusive"),
        tr!("profile-self"),
        tr!("profile-calls")
    ));
    for c in costs.iter().take(60) {
        out.push_str(&format!(
            "| `{}` | {:.2} ms | {:.2} ms | {} |\n",
            c.name.replace('|', "\\|"),
            c.inclusive_ms,
            c.self_ms,
            c.calls
        ));
    }
    out
}

/// Profiles `script` with Xdebug through the project's interpreter and
/// writes a Markdown report next to the other scratch files; returns its
/// path.
///
/// # Errors
/// Returns a message when PHP cannot start or Xdebug wrote no profile.
pub async fn profile_script(root: PathBuf, script: PathBuf) -> Result<PathBuf, String> {
    let interpreter = Interpreter::of(&root);
    let dir = interpreter.scratch_dir(&root).join("profile");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let remote_dir = interpreter.to_remote(&root, &dir);
    // Load Xdebug when PHP does not (Herd's builds).
    let mut args: Vec<OsString> = super::xdebug_arguments(&root, &interpreter)
        .await?
        .into_iter()
        .map(OsString::from)
        .collect();
    args.extend::<Vec<OsString>>(vec![
        "-dxdebug.mode=profile".into(),
        "-dxdebug.start_with_request=yes".into(),
        format!("-dxdebug.output_dir={remote_dir}").into(),
        "-dxdebug.profiler_output_name=cachegrind.out.phpthunder".into(),
        script.clone().into_os_string(),
    ]);
    interpreter
        .command(&root, "php", &args)
        .output()
        .await
        .map_err(|e| tr!("error-run-php", error = e.to_string()))?;
    let profile = dir.join("cachegrind.out.phpthunder");
    let text = interpreter
        .read_back(&root, &profile)
        .await
        .map_err(|_| tr!("profile-none"))?;
    if text.is_empty() {
        return Err(tr!("profile-none"));
    }
    let name = script
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let path = dir.join("profile.md");
    std::fs::write(&path, report(&name, &parse_cachegrind(&text))).map_err(|e| e.to_string())?;
    Ok(path)
}

/// One line describing how Xdebug is set up for the project's PHP: its
/// version, mode, client host and port, and whether it starts with every
/// request; or what is missing.
///
/// # Errors
/// Returns a message when PHP cannot start.
pub async fn validate(root: PathBuf) -> Result<String, String> {
    let interpreter = Interpreter::of(&root);
    let code = "if (!extension_loaded('xdebug')) { echo 'missing'; return; } \
        echo phpversion('xdebug'), '|', ini_get('xdebug.mode'), '|', ini_get('xdebug.client_host'), '|', \
        ini_get('xdebug.client_port'), '|', ini_get('xdebug.start_with_request');";
    let out = interpreter
        .command(&root, "php", &["-r".into(), code.into()])
        .output()
        .await
        .map_err(|e| tr!("error-run-php", error = e.to_string()))?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if text == "missing" || text.is_empty() {
        return Ok(tr!("debug-validate-missing", php = interpreter.label()));
    }
    let parts: Vec<&str> = text.split('|').collect();
    let get = |i: usize| parts.get(i).copied().unwrap_or("").to_string();
    Ok(tr!(
        "debug-validate-ok",
        php = interpreter.label(),
        version = get(0),
        mode = get(1),
        host = get(2),
        port = get(3),
        start = get(4)
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROFILE: &str = "version: 1
events: Time_(10ns) Memory_(bytes)

fl=(1) /app/a.php
fn=(1) helper
3 50000 0

fl=(1)
fn=(2) {main}
1 10000 0
cfl=(1)
cfn=(1)
calls=2 0 0
2 50000 0
";

    #[test]
    fn cachegrind_should_split_self_and_inclusive_time() {
        let costs = parse_cachegrind(PROFILE);
        let main = costs.iter().find(|c| c.name == "{main}").unwrap();
        let helper = costs.iter().find(|c| c.name == "helper").unwrap();
        assert_eq!(costs[0].name, "{main}");
        assert!((main.self_ms - 0.1).abs() < 1e-9);
        assert!((main.inclusive_ms - 0.6).abs() < 1e-9);
        assert_eq!(helper.calls, 2);
        assert_eq!(helper.file, "/app/a.php");
    }

    #[test]
    fn resolve_name_should_remember_compressed_names() {
        let mut table = HashMap::new();
        assert_eq!(
            resolve_name(&mut table, "(3) App\\User->save"),
            "App\\User->save"
        );
        assert_eq!(resolve_name(&mut table, "(3)"), "App\\User->save");
        assert_eq!(resolve_name(&mut table, "plain"), "plain");
    }
}
