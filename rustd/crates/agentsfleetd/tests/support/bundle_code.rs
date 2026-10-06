//! The code-running bundle's fixtures: the repository `test-fixer` binds,
//! made with the `git` binary and served over `file://`, and the model that
//! runs its suite, fixes the script with a patch, runs it again and commits.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use std::fs;
use std::path::Path;
use std::process::Command;

use afr_providers::Chunk;
use serde_json::json;

use crate::fake_model::{call, say};

/// The bundle under test, and the repository it binds.
pub(crate) const BUNDLE: &str = "test-fixer";
pub(crate) const REPOSITORY: &str = "agentsfleet/greeter";
/// The script under test, wrong by one letter, and the suite that catches it.
const GREET: &str = "echo helo\n";
const SUITE: &str = "got=\"$(sh ./greet.sh)\"\n\
                     if [ \"$got\" = hello ]; then echo PASS; else echo \"FAIL: got $got\"; exit 1; fi\n";
/// What the suite prints when it fails, and when it passes.
pub(crate) const FAILED: &str = "FAIL: got helo";
pub(crate) const PASSED: &str = "PASS";
/// The suite, run from the workspace root, where `shell` starts.
const RUN_SUITE: &str = "cd greeter && sh test.sh";
/// The patch that fixes the script, by its path from the workspace root.
pub(crate) const FIX: &str = "*** Begin Patch\n\
                              *** Update File: greeter/greet.sh\n\
                              @@\n\
                              -echo helo\n\
                              +echo hello\n\
                              *** End Patch\n";
/// The commit's subject, and what the fleet answers.
pub(crate) const SUBJECT: &str = "fix the greeting";
pub(crate) const ANSWER: &str = "The greeting printed helo; greet.sh now prints hello, and the \
                                 suite exits 0. Committed as \"fix the greeting\".";
/// The branch the origin's one commit is on.
const DEFAULT_BRANCH: &str = "main";
/// Who the origin's commit names.
const FIXTURE_IDENTITY: [(&str, &str); 4] = [
    ("GIT_AUTHOR_NAME", "fixture"),
    ("GIT_AUTHOR_EMAIL", "fixture@example.com"),
    ("GIT_COMMITTER_NAME", "fixture"),
    ("GIT_COMMITTER_EMAIL", "fixture@example.com"),
];

/// Runs `git` in `dir` with no configuration from this host.
fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .envs(FIXTURE_IDENTITY)
        .status()
        .expect("git runs");
    assert!(status.success(), "git {args:?} failed");
}

/// A bare [`REPOSITORY`] under `origins` with one commit holding the script
/// and its suite, on [`DEFAULT_BRANCH`]; the origin a runner fetches it from.
pub(crate) fn greeter_origin(origins: &Path) -> String {
    let work = origins.join("work");
    fs::create_dir_all(&work).expect("a working directory");
    fs::write(work.join("greet.sh"), GREET).expect("the script writes");
    fs::write(work.join("test.sh"), SUITE).expect("the suite writes");
    git(
        &work,
        &["init", "--quiet", "--initial-branch", DEFAULT_BRANCH],
    );
    git(&work, &["add", "."]);
    git(&work, &["commit", "--quiet", "--message", "greet"]);
    let bare = origins.join(format!("{REPOSITORY}.git"));
    git(
        origins,
        &[
            "clone",
            "--quiet",
            "--bare",
            &work.to_string_lossy(),
            &bare.to_string_lossy(),
        ],
    );
    format!("file://{}/", origins.display())
}

/// The fixer's run, as `test-fixer/SKILL.md` orders it: the suite, the patch,
/// the suite again, the commit, the answer.
pub(crate) fn fixer_turns() -> Vec<Vec<Chunk>> {
    vec![
        vec![call("suite", "shell", json!({"command": RUN_SUITE}))],
        vec![call("fix", "apply_patch", json!({"patch": FIX}))],
        vec![call("again", "shell", json!({"command": RUN_SUITE}))],
        vec![call(
            "commit",
            "git",
            json!({"args": ["commit", "--all", "--message", SUBJECT]}),
        )],
        vec![say(ANSWER)],
    ]
}
