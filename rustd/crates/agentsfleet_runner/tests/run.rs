//! `run` end to end, as the binary itself: pointed at a fake daemon, it takes
//! one lease through the real agent loop and settles it, and with a collector
//! configured it exports that lease's trace under this host's identity.
//!
//! The binary runs `--unsandboxed`, which only a debug build accepts, so this
//! suite is debug-only; the sandbox itself is the kernel lane's to prove. The
//! lease names a model under a domain that never resolves, so its turn reaches
//! the real provider and fails there, and the report is a run that ended, not
//! one refused before it started.
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    reason = "test target: a binary that will not serve should fail the test loudly"
)]

use std::io::Read as _;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use afd_observability::semconv::{ATTR_LEASE_ID, RESOURCE_SERVICE_INSTANCE_ID, SPAN_RUNNER_LEASE};
use afd_otlp::config::{OTEL_ENDPOINT_KNOB, OTEL_PROTOCOL_KNOB};
use afd_otlp::resource::INSTANCE_ID_KNOB;
use serde_json::Value;

use crate::fake_daemon::{ALLOW_ALL, FakeCollector, FakeDaemon, LEASE_ID};
use crate::support::{RUN_FAILED, Runner};

/// The fencing token the fixture lease carries, which the report echoes.
const FENCING: u64 = 504;
/// How long one lease is given to settle.
const SETTLE_WITHIN: Duration = Duration::from_secs(60);
/// How long the binary is given to stop once told to, as systemd tells it.
const STOP_WITHIN: Duration = Duration::from_secs(30);
/// How often a stopping binary is checked.
const POLL: Duration = Duration::from_millis(50);
/// The class of a lease refused before its turn ran.
const STARTUP_POSTURE: &str = "startup_posture";
/// The encoding the fake collector reads.
const JSON_PROTOCOL: &str = "http/json";
/// Where OTLP/HTTP trace exports are posted.
const TRACES: &str = "/v1/traces";
/// The identity the systemd unit gives a host (`%H`), here spelled out.
const INSTANCE: &str = "host-a";
/// The other two egress postures a daemon assigns.
const DENY_ALL: &str = "deny_all_egress";
const ALLOW_LIST: &str = "allow_list_egress";
/// A registry every host resolves offline, so the allowlist binds without
/// the network.
const LOCAL_REGISTRY: &str = "localhost";
/// What a lease whose egress would not bind logs.
const EGRESS_REFUSED: &str = "egress_bind_refused";

/// One run of the binary through one lease.
struct Ran {
    status: ExitStatus,
    stderr: String,
    report: Value,
}

/// Starts `run --unsandboxed` against `daemon` with `extra` in its
/// environment, waits for its report, then stops it with SIGTERM.
fn run_one_lease(
    runtime: &tokio::runtime::Runtime,
    daemon: &mut FakeDaemon,
    extra: &[(&str, &str)],
) -> Ran {
    // Under /tmp, short: each lease's executor socket is made beneath it, and
    // a Unix socket's path is capped near a hundred bytes.
    let home = tempfile::Builder::new()
        .prefix("afr-run-")
        .tempdir_in("/tmp")
        .expect("a storage home");
    let mut child = Runner::new(&["run", "--unsandboxed"])
        .daemon(&daemon.url)
        .home(home.path())
        .envs(extra)
        .command()
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the runner binary starts");
    let stderr = drain_stderr(&mut child);

    let report = runtime.block_on(daemon.report(SETTLE_WITHIN));
    terminate(&child);
    let status = stop(&mut child);
    let stderr = stderr.join().expect("the stderr reader joins");
    let report = report.unwrap_or_else(|| panic!("no report within {SETTLE_WITHIN:?}: {stderr}"));
    Ran {
        status,
        stderr,
        report,
    }
}

/// Reads the binary's stderr on a thread of its own, so a chatty run never
/// blocks on a full pipe.
fn drain_stderr(child: &mut Child) -> JoinHandle<String> {
    let mut pipe = child.stderr.take().expect("stderr is piped");
    std::thread::spawn(move || {
        let mut text = String::new();
        drop(pipe.read_to_string(&mut text));
        text
    })
}

/// Sends SIGTERM, as systemd's stop does.
///
/// Through `kill(1)`: this test crate declares no `libc` dependency and writes
/// no unsafe code.
fn terminate(child: &Child) {
    let sent = Command::new("kill")
        .args(["-TERM", &child.id().to_string()])
        .status()
        .expect("kill(1) runs");
    assert!(sent.success(), "SIGTERM was delivered to the runner");
}

/// Waits for the binary to exit, killing it past [`STOP_WITHIN`].
fn stop(child: &mut Child) -> ExitStatus {
    let deadline = Instant::now() + STOP_WITHIN;
    loop {
        if let Some(status) = child.try_wait().expect("the runner's status reads") {
            return status;
        }
        if Instant::now() > deadline {
            drop(child.kill());
            panic!("the runner did not stop within {STOP_WITHIN:?} of SIGTERM");
        }
        std::thread::sleep(POLL);
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Runtime::new().expect("a runtime for the fakes")
}

/// The string value of `key` among an OTLP `attributes` array.
fn attribute<'a>(holder: &'a Value, key: &str) -> Option<&'a str> {
    holder["attributes"]
        .as_array()?
        .iter()
        .find(|attribute| attribute["key"] == key)?
        .pointer("/value/stringValue")?
        .as_str()
}

/// The built binary against a daemon granting one lease: the lease is
/// admitted, its turn runs on the agent loop, one report settles it, and the
/// binary stops cleanly on SIGTERM with nothing logged as a failure.
#[test]
fn test_runner_binary_runs_a_lease() {
    let runtime = runtime();
    let mut daemon = runtime.block_on(FakeDaemon::start());

    let ran = run_one_lease(&runtime, &mut daemon, &[]);

    assert_eq!(ran.report["lease_id"], LEASE_ID, "{}", ran.report);
    assert_eq!(ran.report["fencing_token"], FENCING, "{}", ran.report);
    assert_ne!(
        ran.report["failure_reason"], STARTUP_POSTURE,
        "the lease was admitted and its turn ran: {}",
        ran.report
    );
    assert!(ran.status.success(), "{:?}: {}", ran.status, ran.stderr);
    assert!(!ran.stderr.contains(RUN_FAILED), "{}", ran.stderr);
}

/// The same lease with a collector configured: its root span reaches the
/// collector naming the lease, under the instance identity the systemd unit
/// sets, so each host publishes its own series.
#[test]
fn test_runner_binary_exports_its_lease_trace() {
    let runtime = runtime();
    let mut daemon = runtime.block_on(FakeDaemon::start());
    let collector = runtime.block_on(FakeCollector::start());

    let ran = run_one_lease(
        &runtime,
        &mut daemon,
        &[
            (OTEL_ENDPOINT_KNOB, &collector.url),
            (OTEL_PROTOCOL_KNOB, JSON_PROTOCOL),
            (INSTANCE_ID_KNOB, INSTANCE),
        ],
    );

    assert!(ran.status.success(), "{:?}: {}", ran.status, ran.stderr);
    let resources: Vec<Value> = collector
        .posted_to(TRACES)
        .iter()
        .filter_map(|export| export["resourceSpans"].as_array().cloned())
        .flatten()
        .collect();
    let lease = resources.iter().find_map(|resource| {
        resource["scopeSpans"]
            .as_array()?
            .iter()
            .filter_map(|scope| scope["spans"].as_array())
            .flatten()
            .find(|span| span["name"] == SPAN_RUNNER_LEASE)
            .map(|span| (resource, span))
    });
    let (resource, root) =
        lease.unwrap_or_else(|| panic!("no lease span reached the collector: {}", ran.stderr));
    assert_eq!(attribute(root, ATTR_LEASE_ID), Some(LEASE_ID), "{root}");
    assert_eq!(
        attribute(&resource["resource"], RESOURCE_SERVICE_INSTANCE_ID),
        Some(INSTANCE),
        "{resource}"
    );
}

/// The built binary settles a lease under each egress posture the daemon
/// assigns: the allowlist binds through the host's resolver, and no posture
/// refuses the lease before its turn runs.
#[test]
fn test_runner_binary_runs_a_lease_per_policy() {
    let runtime = runtime();
    for (policy, registry) in [
        (ALLOW_ALL, &[][..]),
        (DENY_ALL, &[][..]),
        (ALLOW_LIST, &[LOCAL_REGISTRY][..]),
    ] {
        let mut daemon = runtime.block_on(FakeDaemon::assigning(policy, registry));

        let ran = run_one_lease(&runtime, &mut daemon, &[]);

        assert_eq!(ran.report["lease_id"], LEASE_ID, "{policy}: {}", ran.report);
        assert_ne!(
            ran.report["failure_reason"], STARTUP_POSTURE,
            "{policy}: the lease was admitted and its turn ran: {}",
            ran.report
        );
        assert!(
            ran.status.success(),
            "{policy}: {:?}: {}",
            ran.status,
            ran.stderr
        );
        assert!(!ran.stderr.contains(RUN_FAILED), "{policy}: {}", ran.stderr);
        assert!(
            !ran.stderr.contains(EGRESS_REFUSED),
            "{policy}: {}",
            ran.stderr
        );
    }
}
