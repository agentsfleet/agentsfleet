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

/// Where an instrumented build writes its coverage profile. Kept across a
/// cleared environment, so the binary's own lines are measured when the suite
/// runs under coverage; absent, it changes nothing.
const PROFILE_KNOB: &str = "LLVM_PROFILE_FILE";

/// `word` with an empty environment, but for the coverage profile's path.
fn cleared(word: &str) -> Command {
    let mut command = Command::new(BINARY);
    command.arg(word).env_clear();
    if let Some(profile) = std::env::var_os(PROFILE_KNOB) {
        command.env(PROFILE_KNOB, profile);
    }
    command
}

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

/// With a valid environment, `run` boots — storage home opened, host probed —
/// and refuses before it contacts the daemon: exit 2 where the
/// host could build a sandbox, 1 where it could not, `run_refused` either way.
#[test]
fn run_boots_then_refuses_without_an_agent_engine() {
    let home = tempfile::tempdir().expect("a storage home");
    let ran = cleared("run")
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
        home.path().join("sandboxes").is_dir(),
        "the storage home was opened, its directories made"
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

/// Where a runner is told to export.
const ENDPOINT_KNOB: &str = "OTEL_EXPORTER_OTLP_ENDPOINT";

/// The knob that would hand a runner a credential.
const HEADERS_KNOB: &str = "OTEL_EXPORTER_OTLP_HEADERS";

/// A collector that refuses every connection, promptly.
const REFUSING_COLLECTOR: &str = "http://127.0.0.1:1";

/// What `run` logs when it exports, and when it does not.
const EXPORT_STARTED: &str = "telemetry_export_started";
const EXPORT_DISABLED: &str = "telemetry_export_disabled";

/// `run` against a valid environment plus `extra`, its storage home under
/// `home`.
fn run_with(home: &std::path::Path, extra: &[(&str, &str)]) -> Output {
    cleared("run")
        .env(ENV_API_URL, UNREACHABLE_DAEMON)
        .env(ENV_RUNNER_TOKEN, TOKEN)
        .env(ENV_STORAGE_HOME, home)
        .envs(extra.iter().copied())
        .output()
        .expect("the runner binary starts")
}

/// The exit status a valid `run` ends with on this host.
fn booted_status() -> i32 {
    let host_can_sandbox = afr_sandbox::probe(&afr_sandbox::ProbePaths::default())
        .missing()
        .is_none();
    if host_can_sandbox { REFUSED } else { 1 }
}

/// Without the endpoint, `run` builds no export and says so once.
#[test]
fn test_runner_exports_nothing_when_unconfigured() {
    let home = tempfile::tempdir().expect("a storage home");

    let ran = run_with(home.path(), &[]);

    let stderr = String::from_utf8_lossy(&ran.stderr);
    assert_eq!(stderr.matches(EXPORT_DISABLED).count(), 1, "{stderr}");
    assert!(!stderr.contains(EXPORT_STARTED), "{stderr}");
    assert_eq!(ran.status.code(), Some(booted_status()), "{stderr}");
}

/// With the endpoint set, `run` exports and names the knob it read, never
/// the collector's address; the boot that follows is unchanged.
#[test]
fn run_exports_naming_only_the_knob() {
    let home = tempfile::tempdir().expect("a storage home");

    let ran = run_with(home.path(), &[(ENDPOINT_KNOB, REFUSING_COLLECTOR)]);

    let stderr = String::from_utf8_lossy(&ran.stderr);
    assert!(
        stderr.contains(EXPORT_STARTED) && stderr.contains(ENDPOINT_KNOB),
        "{stderr}"
    );
    assert!(
        !stderr.contains(REFUSING_COLLECTOR),
        "the address stays out of the journal: {stderr}"
    );
    assert!(stderr.contains(RUN_REFUSED), "{stderr}");
    assert_eq!(ran.status.code(), Some(booted_status()), "{stderr}");
}

/// A header knob refuses `run` before it boots, naming the knob: the runner
/// carries no credential.
#[test]
fn run_refuses_a_credential_naming_the_knob() {
    let home = tempfile::tempdir().expect("a storage home");

    let ran = run_with(
        home.path(),
        &[
            (ENDPOINT_KNOB, REFUSING_COLLECTOR),
            (HEADERS_KNOB, "authorization=Bearer x"),
        ],
    );

    let stderr = String::from_utf8_lossy(&ran.stderr);
    assert_eq!(ran.status.code(), Some(1), "{stderr}");
    assert!(
        stderr.contains(RUN_FAILED) && stderr.contains(HEADERS_KNOB),
        "{stderr}"
    );
    assert!(
        !stderr.contains("Bearer"),
        "the refused value is never echoed: {stderr}"
    );
    assert!(
        !home.path().join("sandboxes").is_dir(),
        "refused before boot opened the storage home"
    );
}

/// `sandbox` with the endpoint set behaves exactly as without it: it reads no
/// telemetry knob and starts no export thread before hardening, so whatever
/// it refuses, it refuses the same way and never for a second thread.
#[test]
fn test_sandbox_hardens_with_telemetry_configured() {
    let sandbox = |extra: &[(&str, &str)]| {
        Command::new(BINARY)
            .arg("sandbox")
            .env_remove(afd_core::env::LOG_LEVEL_VAR)
            .envs(extra.iter().copied())
            .output()
            .expect("the runner binary starts")
    };

    let plain = sandbox(&[]);
    let exporting = sandbox(&[(ENDPOINT_KNOB, REFUSING_COLLECTOR)]);

    assert_eq!(plain.status.code(), exporting.status.code());
    assert_eq!(
        String::from_utf8_lossy(&plain.stderr),
        String::from_utf8_lossy(&exporting.stderr),
        "the endpoint changes nothing the sandbox entry does"
    );
    let stderr = String::from_utf8_lossy(&exporting.stderr);
    assert!(!stderr.contains("second thread"), "{stderr}");
}
