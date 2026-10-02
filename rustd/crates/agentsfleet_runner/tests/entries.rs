//! The binary's three entries, run as the binary itself.
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test target: a binary that will not start should fail the test loudly"
)]

use std::process::{Command, Output};

use afr_supervisor::config::{ENV_API_URL, ENV_RUNNER_TOKEN, ENV_STORAGE_HOME};

/// The built binary under test.
const BINARY: &str = env!("CARGO_BIN_EXE_agentsfleet-runner");

/// What `run` logs when it refuses to start.
const RUN_REFUSED: &str = "run_refused";

/// What `run` logs when its boot fails.
const RUN_FAILED: &str = "run_failed";

/// A daemon address nothing answers on; `run` must refuse before dialling it.
const UNREACHABLE_DAEMON: &str = "http://127.0.0.1:9";

/// A token of the runner's shape.
const TOKEN: &str = "agt_r_entries_test";

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

/// Without its environment, `run` fails at boot and names why.
#[test]
fn run_fails_at_boot_without_its_environment() {
    let ran = entry("run");

    assert_eq!(ran.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&ran.stderr).contains(RUN_FAILED),
        "{}",
        String::from_utf8_lossy(&ran.stderr)
    );
}

/// With a valid environment, `run` boots — storage home opened and swept,
/// host probed — and refuses before it contacts the daemon: exit 2 where the
/// host could build a sandbox, 1 where it could not, `run_refused` either way.
#[test]
fn run_boots_then_refuses_without_an_agent_engine() {
    let home = tempfile::tempdir().expect("a storage home");
    let ran = Command::new(BINARY)
        .arg("run")
        .env_clear()
        .env(ENV_API_URL, UNREACHABLE_DAEMON)
        .env(ENV_RUNNER_TOKEN, TOKEN)
        .env(ENV_STORAGE_HOME, home.path())
        .output()
        .expect("the runner binary starts");

    let stderr = String::from_utf8_lossy(&ran.stderr);
    assert!(stderr.contains(RUN_REFUSED), "{stderr}");
    let host_can_sandbox = afr_sandbox::probe(&afr_sandbox::ProbePaths::default())
        .missing()
        .is_none();
    let expected = if host_can_sandbox { REFUSED } else { 1 };
    assert_eq!(ran.status.code(), Some(expected), "{stderr}");
    assert!(
        home.path().exists(),
        "the storage home was opened, not removed"
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
