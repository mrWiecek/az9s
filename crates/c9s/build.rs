//! Embed the git commit in the binary so `c9s --version` identifies the
//! exact source of a build (audit trail for bug reports and rollbacks).

use std::path::Path;
use std::process::Command;

fn main() {
    let commit = Command::new("git")
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=C9S_COMMIT={commit}");

    // Re-run when HEAD moves (checkout, new commit); scanning refs/ catches
    // commits on the current branch.
    for path in [
        "../../.git/HEAD",
        "../../.git/refs",
        "../../.git/packed-refs",
    ] {
        if Path::new(path).exists() {
            println!("cargo:rerun-if-changed={path}");
        }
    }
}
