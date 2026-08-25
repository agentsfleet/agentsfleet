//! Stamps the commit this binary was built from into the build environment.
//!
//! `/healthz` reports it so an operator can correlate a running process with a
//! tree. Before this script the field existed and nothing set it, so it read
//! `"commit":"unknown"` in every build ever made — which looks like a broken
//! build rather than a missing wire-up, and is the one thing a diagnostic field
//! must never do.
//!
//! `build_options.git_commit` is the Zig equivalent (`build.zig:36`).

use std::process::Command;

/// Where git records the ref HEAD points at.
const GIT_HEAD: &str = "../../../.git/HEAD";

/// The binary consulted for the commit, named once.
const GIT: &str = "git";

fn main() {
    // Re-run when HEAD moves. Without this cargo caches the stamp and a
    // rebuild after a commit would report the previous one — a wrong answer,
    // which is worse than the `unknown` this replaces.
    println!("cargo:rerun-if-changed={GIT_HEAD}");
    println!("cargo:rerun-if-env-changed=AFD_GIT_COMMIT");

    // An explicit value wins: a container build has no `.git`, and CI knows
    // the SHA it checked out.
    if std::env::var_os("AFD_GIT_COMMIT").is_some() {
        return;
    }

    let described = Command::new(GIT)
        .args(["rev-parse", "--short=12", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .and_then(|out| String::from_utf8(out.stdout).ok());

    // No git, no repository, or a build from a tarball: leave it unset and let
    // the `option_env!` fallback report `unknown` honestly.
    let Some(commit) = described else {
        return;
    };

    let commit = commit.trim();
    if commit.is_empty() {
        return;
    }

    // A dirty tree is marked, because "which commit is running" is a question
    // whose answer is misleading when uncommitted changes are in the binary.
    let dirty = Command::new(GIT)
        .args(["status", "--porcelain", "--untracked-files=no"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .is_some_and(|out| !out.stdout.is_empty());

    let suffix = if dirty { "-dirty" } else { "" };
    println!("cargo:rustc-env=AFD_GIT_COMMIT={commit}{suffix}");
}
