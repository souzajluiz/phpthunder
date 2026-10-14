//! Build script: records the commit the IDE is built from, shown in the
//! About dialog.

use std::process::Command;

/// Runs `git` with `args` in the crate directory; its trimmed output, or
/// `None` when git is missing, fails or prints nothing (e.g. a source
/// tarball without `.git`).
fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    let text = String::from_utf8(output.stdout).ok()?;
    let text = text.trim();
    (output.status.success() && !text.is_empty()).then(|| text.to_owned())
}

/// Exports `PHPTHUNDER_GIT_HASH` (short commit hash, `unknown` outside a
/// checkout) and reruns when HEAD moves, so the hash stays current without
/// rebuilding on every unrelated change.
fn main() {
    let hash = git(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=PHPTHUNDER_GIT_HASH={hash}");
    println!("cargo:rerun-if-changed=build.rs");
    if let Some(dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        // HEAD changes on checkout; logs/HEAD gains a line on every commit.
        println!("cargo:rerun-if-changed={dir}/HEAD");
        println!("cargo:rerun-if-changed={dir}/logs/HEAD");
    }
}
