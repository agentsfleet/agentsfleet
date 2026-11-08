//! Fixture repositories made with the `git` binary, reading no configuration
//! from this host, for the suites that check a repository out over `file://`.

use std::fs;
use std::path::Path;
use std::process::Command;

/// The branch every fixture repository starts on.
pub(crate) const FIXTURE_BRANCH: &str = "main";
/// What the fixture's first commit writes into `README.md`.
pub(crate) const FIRST_README: &str = "first";
/// Who every fixture commit names, author and committer alike.
const FIXTURE_NAME: &str = "fixture";
const FIXTURE_EMAIL: &str = "fixture@example.com";
/// Keeps `git` from narrating what it did.
const QUIET: &str = "--quiet";

/// Runs `git` in `dir` and answers its trimmed output.
pub(crate) fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", FIXTURE_NAME)
        .env("GIT_AUTHOR_EMAIL", FIXTURE_EMAIL)
        .env("GIT_COMMITTER_NAME", FIXTURE_NAME)
        .env("GIT_COMMITTER_EMAIL", FIXTURE_EMAIL)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

/// The commit `rev` names in `repository`.
pub(crate) fn head(repository: &Path, rev: &str) -> String {
    git(repository, &["rev-parse", rev])
}

/// Commits `file` holding `content` on whatever `repository` has checked out.
pub(crate) fn commit(repository: &Path, file: &str, content: &str) {
    fs::write(repository.join(file), content).unwrap();
    git(repository, &["add", file]);
    git(repository, &["commit", QUIET, "--message", content]);
}

/// A repository at `path` with one commit of `README.md` on
/// [`FIXTURE_BRANCH`].
pub(crate) fn repository(path: &Path) {
    fs::create_dir_all(path).unwrap();
    git(path, &["init", QUIET, "--initial-branch", FIXTURE_BRANCH]);
    commit(path, "README.md", FIRST_README);
}
