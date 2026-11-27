//! The binary's entries, run as the binary itself.
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test target: a binary that will not start should fail the test loudly"
)]

use std::process::{Command, Output};

use af_otlp::config::{HEADER_KNOBS, OTEL_ENDPOINT_KNOB};

use crate::support::{BINARY, RUN_FAILED, Runner};

/// A daemon address nothing answers on; `run` must refuse before dialling it.
const UNREACHABLE_DAEMON: &str = "http://127.0.0.1:9";

fn entry(word: &str) -> Output {
    Command::new(BINARY)
        .arg(word)
        .env_remove(af_core::env::LOG_LEVEL_VAR)
        .output()
        .expect("the runner binary starts")
}

/// The checks Firecracker's boot admission refuses a host on.
const ADMITTING: [&str; 4] = [
    "cgroup_controllers",
    "kvm",
    "nftables",
    "toolbox_filesystem",
];

/// `probe` prints the capability report and every check Firecracker relies
/// on, the KVM device and the packet filter among them, and succeeds only
/// where its boot would admit the host.
#[test]
fn probe_answers_with_the_report_and_every_check() {
    let probed = entry("probe");

    let answer: serde_json::Value =
        serde_json::from_slice(&probed.stdout).expect("probe prints JSON");
    let checks = answer["checks"].as_array().expect("probe names its checks");
    assert_eq!(checks.len(), 6, "{answer}");
    for relied_on in ["kvm", "nftables"] {
        assert!(
            checks.iter().any(|check| check["name"] == relied_on),
            "{answer}"
        );
    }
    assert_eq!(
        answer["capability_report"]["engine"], "firecracker",
        "{answer}"
    );
    let admitted = checks
        .iter()
        .filter(|check| ADMITTING.iter().any(|name| check["name"] == *name))
        .all(|check| check["ok"] == true);
    assert_eq!(probed.status.success(), admitted, "{answer}");
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

/// With a valid environment and no staged toolbox, `run` boots — storage
/// home opened, host probed — and stops before it contacts the daemon, saying
/// why: a test host either cannot build a sandbox or has no release staged.
#[test]
fn run_stops_at_boot_on_a_host_it_cannot_run_on() {
    let home = tempfile::tempdir().expect("a storage home");
    let ran = run_with(home.path(), &[]);

    let stderr = String::from_utf8_lossy(&ran.stderr);
    assert!(stderr.contains(RUN_FAILED), "{stderr}");
    assert_eq!(ran.status.code(), Some(1), "{stderr}");
    assert!(
        home.path().join("sandboxes").is_dir(),
        "the storage home was opened, its directories made"
    );
}

/// A collector that refuses every connection, promptly.
const REFUSING_COLLECTOR: &str = "http://127.0.0.1:1";

/// What `run` logs when it exports, and when it does not.
const EXPORT_STARTED: &str = "telemetry_export_started";
const EXPORT_DISABLED: &str = "telemetry_export_disabled";

/// `run` against a valid environment plus `extra`, its storage home under
/// `home`.
fn run_with(home: &std::path::Path, extra: &[(&str, &str)]) -> Output {
    Runner::new(&["run"])
        .daemon(UNREACHABLE_DAEMON)
        .envs(&[(
            afr_supervisor::config::ENV_STREAM_URL,
            "https://localhost:9",
        )])
        .home(home)
        .envs(extra)
        .command()
        .output()
        .expect("the runner binary starts")
}

/// Without the endpoint, `run` builds no export and says so once.
#[test]
fn test_runner_exports_nothing_when_unconfigured() {
    let home = tempfile::tempdir().expect("a storage home");

    let ran = run_with(home.path(), &[]);

    let stderr = String::from_utf8_lossy(&ran.stderr);
    assert_eq!(stderr.matches(EXPORT_DISABLED).count(), 1, "{stderr}");
    assert!(!stderr.contains(EXPORT_STARTED), "{stderr}");
    assert_eq!(ran.status.code(), Some(1), "{stderr}");
}

/// With the endpoint set, `run` exports and names the knob it read, never
/// the collector's address; the boot that follows is unchanged.
#[test]
fn run_exports_naming_only_the_knob() {
    let home = tempfile::tempdir().expect("a storage home");

    let ran = run_with(home.path(), &[(OTEL_ENDPOINT_KNOB, REFUSING_COLLECTOR)]);

    let stderr = String::from_utf8_lossy(&ran.stderr);
    assert!(
        stderr.contains(EXPORT_STARTED) && stderr.contains(OTEL_ENDPOINT_KNOB),
        "{stderr}"
    );
    assert!(
        !stderr.contains(REFUSING_COLLECTOR),
        "the address stays out of the journal: {stderr}"
    );
    assert!(stderr.contains(RUN_FAILED), "{stderr}");
    assert_eq!(ran.status.code(), Some(1), "{stderr}");
}

/// A header knob refuses `run` before it boots, naming the knob: the runner
/// carries no credential. Each signal's own header knob refuses the same way,
/// since the exporter would read it from the environment itself.
#[test]
fn run_refuses_a_credential_naming_the_knob() {
    for knob in HEADER_KNOBS {
        let home = tempfile::tempdir().expect("a storage home");

        let ran = run_with(
            home.path(),
            &[
                (OTEL_ENDPOINT_KNOB, REFUSING_COLLECTOR),
                (knob, "authorization=Bearer x"),
            ],
        );

        let stderr = String::from_utf8_lossy(&ran.stderr);
        assert_eq!(ran.status.code(), Some(1), "{knob}: {stderr}");
        assert!(
            stderr.contains(RUN_FAILED) && stderr.contains(knob),
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
}
