//! The user's shell environment. Apps opened from Finder or the Dock get a
//! minimal `PATH` (`/usr/bin:/bin:…`), so `php`, `composer`, Node tools
//! and version managers installed by the user would not be found; the
//! login shell's `PATH` is imported at start-up.

use std::process::Command;
use std::time::Duration;

/// Longest the login shell may take to report its `PATH`.
const SHELL_TIMEOUT: Duration = Duration::from_secs(3);

/// Whether `PATH` looks like the launchd default rather than a terminal's.
fn path_is_minimal(path: &str) -> bool {
    !path.split(':').any(|p| {
        p.contains("/usr/local")
            || p.contains("homebrew")
            || p.contains(".cargo")
            || p.contains("Herd")
            || p.contains(".local")
    })
}

/// The `PATH` an interactive login shell would have; `None` when the shell
/// fails or takes too long.
fn login_shell_path() -> Option<String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
    let mut child = Command::new(shell)
        .args(["-ilc", "printf '__PATH__%s' \"$PATH\""])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let started = std::time::Instant::now();
    loop {
        if child.try_wait().ok()?.is_some() {
            break;
        }
        if started.elapsed() > SHELL_TIMEOUT {
            let _ = child.kill();
            return None;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let out = child.wait_with_output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let path = text.rsplit("__PATH__").next()?.trim().to_string();
    (!path.is_empty()).then_some(path)
}

/// Replaces a minimal `PATH` with the login shell's. Called first thing in
/// `main`, before any thread exists, which is when changing the process
/// environment is sound.
pub fn import_login_path() {
    let current = std::env::var("PATH").unwrap_or_default();
    if !cfg!(target_os = "macos") || !path_is_minimal(&current) {
        return;
    }
    if let Some(path) = login_shell_path() {
        // SAFETY: called at the start of `main`, before any other thread
        // is spawned, so nothing reads the environment concurrently.
        unsafe { std::env::set_var("PATH", path) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_is_minimal_should_spot_the_launchd_default() {
        assert!(path_is_minimal("/usr/bin:/bin:/usr/sbin:/sbin"));
        assert!(!path_is_minimal("/opt/homebrew/bin:/usr/bin:/bin"));
    }
}
