//! Inject the git short SHA and commit date into the binary at build time so
//! `stedi --version` reports exactly which build is running — invaluable in bug
//! reports. Zero-dependency: shells out to `git`, with graceful fallbacks so the
//! build never fails outside a git checkout (e.g. a crates.io source tarball).

use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

fn main() {
    let sha = git(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".into());
    let date =
        git(&["log", "-1", "--date=short", "--format=%cd"]).unwrap_or_else(|| "unknown".into());

    println!("cargo:rustc-env=STEDI_GIT_SHA={sha}");
    println!("cargo:rustc-env=STEDI_BUILD_DATE={date}");
    // Rebuild the version string when HEAD moves.
    println!("cargo:rerun-if-changed=.git/HEAD");
}
