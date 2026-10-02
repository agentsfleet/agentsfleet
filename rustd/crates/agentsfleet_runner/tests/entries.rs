//! The binary's three entries, run as the binary itself.
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test target: a binary that will not start should fail the test loudly"
)]

use std::process::{Command, Output};

/// The built binary under test.
const BINARY: &str = env!("CARGO_BIN_EXE_agentsfleet-runner");

/// What `run` logs when it refuses to start.
const RUN_REFUSED: &str = "run_refused";

/// The exit status of an entry this build refuses.
const REFUSED: i32 = 2;

fn entry(word: &str) -> Output {
    Command::new(BINARY)
        .arg(word)
        .env_remove(afd_core::env::LOG_LEVEL_VAR)
        .output()
        .expect("the runner binary starts")
}

/// `probe` prints the capability report and every named check, and succeeds
/// only where the host can build a sandbox.
#[test]
fn probe_answers_with_the_report_and_every_check() {
    let probed = entry("probe");

    let answer: serde_json::Value =
        serde_json::from_slice(&probed.stdout).expect("probe prints JSON");
    let checks = answer["checks"].as_array().expect("probe names its checks");
    assert_eq!(checks.len(), 6, "{answer}");
    assert!(answer["capability_report"].is_object(), "{answer}");
    let all_mechanisms = checks
        .iter()
        .filter(|check| check["name"] != "kvm")
        .all(|check| check["ok"] == true);
    assert_eq!(probed.status.success(), all_mechanisms, "{answer}");
}

/// `run` refuses before it contacts anything, naming why, with the status a
/// service manager does not restart.
#[test]
fn run_refuses_without_an_agent_engine() {
    let ran = entry("run");

    assert_eq!(ran.status.code(), Some(REFUSED));
    assert!(
        String::from_utf8_lossy(&ran.stderr).contains(RUN_REFUSED),
        "{}",
        String::from_utf8_lossy(&ran.stderr)
    );
}

/// Outside a sandbox there is no executor socket to bind, and on a host that
/// cannot harden there is not even that far to go: `sandbox` never serves.
#[test]
fn sandbox_refuses_outside_a_sandbox() {
    let served = entry("sandbox");

    assert!(!served.status.success());
    assert!(!served.stderr.is_empty(), "the refusal says why");
}
