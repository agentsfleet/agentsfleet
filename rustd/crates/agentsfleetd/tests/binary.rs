//! Dimension 7.4, at the boundary that actually matters: the process itself.
//!
//! Everything else in this crate's suites asserts on a returned value. An exit
//! CODE is not a returned value — it is the only thing an init system reads,
//! and the only way to observe it is to run the binary. So these spawn it.
//!
//! `CARGO_BIN_EXE_agentsfleetd` is set by Cargo for integration tests of a
//! package that ships a binary, so the path is the freshly built one rather
//! than whatever is on `PATH`.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::process::{Command, Output};

mod support;

/// The binary under test, built by Cargo for this suite.
const DAEMON: &str = env!("CARGO_BIN_EXE_agentsfleetd");

/// The API role's Postgres knob.
const DATABASE_KNOB: &str = "DATABASE_URL_API";

/// The API role's Redis knob.
const REDIS_KNOB: &str = "REDIS_URL_API";

/// The master-key knob.
const KEK_KNOB: &str = "ENCRYPTION_MASTER_KEY";

/// A Postgres URL the resolver accepts.
const GOOD_DATABASE: &str = "postgres://afd:afd@127.0.0.1:5432/agentsfleet";

/// A Redis URL the resolver accepts.
const GOOD_REDIS: &str = "redis://127.0.0.1:6379";

/// Sixty-four hex characters.
const GOOD_KEK: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

/// Runs the daemon with exactly the knobs given and nothing inherited.
///
/// The inherited environment is stripped knob by knob: a developer with a real
/// `ENCRYPTION_MASTER_KEY` exported would otherwise turn the negative cases
/// green on their machine and nowhere else.
fn run(knobs: &[(&str, &str)]) -> Output {
    let mut command = Command::new(DAEMON);
    for knob in [DATABASE_KNOB, REDIS_KNOB, KEK_KNOB]
        .into_iter()
        .chain(support::SESSION_PEPPER.map(|(knob, _value)| knob))
        .chain(support::IDENTITY_KNOBS)
    {
        command.env_remove(knob);
    }
    for &(knob, value) in knobs {
        command.env(knob, value);
    }
    command.output().expect("the daemon binary runs")
}

/// Dimension 7.4 — an absent or malformed key exits non-zero, naming the knob.
#[test]
fn test_boot_refuses_bad_kek() {
    for bad in [None, Some(""), Some("abcd"), Some("zz")] {
        let mut knobs = vec![(DATABASE_KNOB, GOOD_DATABASE), (REDIS_KNOB, GOOD_REDIS)];
        knobs.extend_from_slice(&support::IDENTITY);
        if let Some(value) = bad {
            knobs.push((KEK_KNOB, value));
        }

        let output = run(&knobs);
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert!(
            !output.status.success(),
            "a daemon that cannot decrypt anything must not exit 0 (key: {bad:?})"
        );
        assert_eq!(
            output.status.code(),
            Some(1),
            "the refusal exit status is 1, which is what an init system restarts on"
        );
        assert!(
            stderr.contains(KEK_KNOB),
            "the operator must be told WHICH knob to fix, got: {stderr}"
        );
        assert!(
            stderr.contains("We flunked!"),
            "the fatal renderer is the one that speaks on the way out, got: {stderr}"
        );
        // Narrowed, not dropped. The nameplate now prints before anything is
        // resolved, so a refusing process DOES write its own identity to
        // stdout — but the thing this guarded is still guarded: no boot
        // RESULT reaches stdout. The refusal, the address and the resolved
        // roles all remain stderr's alone, and stdout carries the nameplate
        // and nothing besides.
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert_eq!(
            stdout.trim(),
            format!("agentsfleetd {}", env!("CARGO_PKG_VERSION")),
            "a refusing process announces its identity and no more"
        );
        for leaked in ["We flunked!", KEK_KNOB, "listening", "postgres:", "redis:"] {
            assert!(
                !stdout.contains(leaked),
                "a boot result must not reach stdout: {leaked} in {stdout}"
            );
        }
    }
}

/// Dimension 8.1, at the process boundary — every unset knob in one run.
#[test]
fn test_preflight_lists_missing() {
    let output = run(&[]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(!output.status.success(), "an empty environment cannot boot");
    for knob in [DATABASE_KNOB, REDIS_KNOB, KEK_KNOB]
        .into_iter()
        .chain(support::SESSION_PEPPER.map(|(knob, _value)| knob))
        .chain(support::IDENTITY.map(|(knob, _value)| knob))
    {
        assert!(
            stderr.contains(knob),
            "one run must name every unset knob; {knob} is absent from: {stderr}"
        );
    }
}

/// A complete environment boots far enough to announce itself, and exits 0.
///
/// No datastore is running for this: preflight resolves settings, it does not
/// connect, and that ordering is the point — a key that cannot work is refused
/// before anything opens a socket.
#[test]
fn test_boot_announces_itself_when_the_environment_is_complete() {
    let mut knobs = vec![
        (DATABASE_KNOB, GOOD_DATABASE),
        (REDIS_KNOB, GOOD_REDIS),
        (KEK_KNOB, GOOD_KEK),
    ];
    knobs.extend_from_slice(&support::SESSION_PEPPER);
    knobs.extend_from_slice(&support::IDENTITY);
    let output = run(&knobs);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "a complete environment boots: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("agentsfleetd"),
        "the banner names the product"
    );
    assert!(
        stdout.contains(env!("CARGO_PKG_VERSION")),
        "and the build, so a log says which one came up"
    );
    assert!(
        stdout.contains("postgres:api") && stdout.contains("redis:api"),
        "and which roles resolved, qualified by datastore: {stdout}"
    );
    assert!(
        !stdout.contains("We flunked!"),
        "the bomb is for the exit path only"
    );
}

/// `--help` carries the nameplate above the usage text.
///
/// The help path never reaches `main`'s own print: clap renders help and exits
/// inside its parse. So this is served by `before_help`, and the test is at the
/// process boundary because that is the only place the two writes can be seen
/// as one page.
#[test]
fn test_help_carries_the_nameplate_above_the_usage() {
    for flag in ["-h", "--help"] {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_agentsfleetd"))
            .arg(flag)
            .output()
            .expect("the daemon binary runs");

        let stdout = String::from_utf8_lossy(&output.stdout);
        let nameplate = format!("agentsfleetd {}", env!("CARGO_PKG_VERSION"));

        assert!(
            output.status.success(),
            "{flag} is not an error: {:?}",
            output.status.code()
        );
        assert!(stdout.contains(&nameplate), "{flag}: {stdout}");
        assert!(stdout.contains("Usage:"), "{flag}: {stdout}");
        assert!(
            stdout.find(&nameplate) < stdout.find("Usage:"),
            "the nameplate comes first: {stdout}"
        );
        // The help page announces the flags that control it.
        assert!(stdout.contains("--no-banner"), "{flag}: {stdout}");
        assert!(stdout.contains("--quiet"), "{flag}: {stdout}");
    }
}
