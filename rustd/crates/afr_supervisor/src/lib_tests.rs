#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use afd_core::id::Uuid7;
use afd_wire::lease::LeaseResponse;
use afd_wire::memory::MemoryHydrateResponse;
use afd_wire::report::{FailureClass, ReportResponse};
use afr_sandbox::{HostProbe, Kvm, Limits};
use tokio_util::sync::CancellationToken;

use super::{Config, Runner, run, serve};
use crate::client::{Call, Verb};
use crate::error;
use crate::report::{Ending, report};
use crate::report_spool::ReportSpool;
use crate::storage_home::StorageHome;

/// The cadence and retry delay the fake daemon answers with, in milliseconds.
const INTERVAL_MS: u32 = 1000;
/// A heartbeat status that keeps the runner working.
const STATUS_OK: &str = "ok";
use crate::test_support::{
    Answer, Behaviour, FLEET_ID, FakeAgent, FakeEngine, LEASE_ID, drain, json, lease, plane,
};

fn probe() -> HostProbe {
    HostProbe {
        landlock: true,
        seccomp: true,
        cgroup_controllers: vec!["cpu".to_owned()],
        bubblewrap: true,
        kvm: Kvm::Absent,
        toolbox_filesystem: true,
    }
}

fn beat(status: &str) -> Answer {
    json(
        &serde_json::json!({"status": status, "assigned_policy": null, "degraded": false,
        "degraded_reason": null, "selftest_requested": false, "heartbeat_interval_ms": INTERVAL_MS}),
    )
}

fn runner(
    answer: impl Fn(&Call) -> Answer + Send + Sync + 'static,
    home: StorageHome,
) -> (Runner, tokio::sync::mpsc::UnboundedReceiver<Call>) {
    let (plane, calls) = plane(answer);
    let runner = Runner {
        plane,
        home,
        engine: Box::new(FakeEngine::default()),
        agent: Box::new(FakeAgent::new(Behaviour::Answer)),
        probe: probe(),
        limits: Limits::default(),
    };
    (runner, calls)
}

/// A report left behind by a process that died before posting it.
fn leave_a_report(home: &StorageHome) {
    let ending = Ending::Failed {
        class: FailureClass::RunnerCrash,
        detail: "killed",
    };
    ReportSpool::new(home)
        .hold(
            &Uuid7::parse(LEASE_ID).unwrap(),
            &report(&lease(LEASE_ID, FLEET_ID, None), &ending, Duration::ZERO),
        )
        .unwrap();
}

/// A daemon that grants one lease, then none, and counts the reports it takes.
fn one_lease_daemon(counted: Arc<AtomicUsize>) -> impl Fn(&Call) -> Answer + Send + Sync + 'static {
    let polled = AtomicUsize::new(0);
    move |call| match call.verb {
        Verb::Heartbeat => beat(STATUS_OK),
        Verb::Lease if polled.fetch_add(1, Ordering::SeqCst) == 0 => json(&LeaseResponse {
            lease: Some(lease(
                "01890a5d-ac96-774b-bcce-b302099a8062",
                FLEET_ID,
                None,
            )),
            retry_after_ms: None,
        }),
        Verb::Lease => json(&LeaseResponse {
            lease: None,
            retry_after_ms: Some(INTERVAL_MS),
        }),
        Verb::Hydrate => json(&MemoryHydrateResponse { memory: Vec::new() }),
        Verb::Report => {
            counted.fetch_add(1, Ordering::SeqCst);
            json(&ReportResponse { ok: true })
        }
        // pin test: literal is the contract
        _ => json(&serde_json::json!({"ok": true, "stored": 1, "skipped": 0})),
    }
}

#[tokio::test(start_paused = true)]
async fn a_runner_sweeps_replays_beats_and_runs_until_shutdown() {
    let root = tempfile::tempdir().unwrap();
    let home = StorageHome::open(root.path()).unwrap();
    fs::create_dir_all(home.lease_dir(&Uuid7::parse(FLEET_ID).unwrap())).unwrap();
    leave_a_report(&home);
    let reports = Arc::new(AtomicUsize::new(0));
    let (runner, mut calls) = runner(one_lease_daemon(Arc::clone(&reports)), home.clone());
    let shutdown = CancellationToken::new();
    let serving = tokio::spawn(serve(runner, shutdown.clone()));

    while reports.load(Ordering::SeqCst) < 2 {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    shutdown.cancel();
    serving.await.unwrap().unwrap();

    let verbs: Vec<_> = drain(&mut calls)
        .into_iter()
        .map(|call| call.verb)
        .collect();
    assert_eq!(
        verbs[0],
        Verb::Report,
        "the spooled report is replayed before the first beat"
    );
    assert_eq!(verbs[1], Verb::Heartbeat);
    assert!(fs::read_dir(home.spool()).unwrap().next().is_none());
    assert!(
        !home.lease_dir(&Uuid7::parse(FLEET_ID).unwrap()).exists(),
        "the orphan was swept"
    );
}

#[tokio::test(start_paused = true)]
async fn a_stop_on_the_first_beat_serves_nothing_and_an_unposted_replay_waits() {
    let root = tempfile::tempdir().unwrap();
    let home = StorageHome::open(root.path()).unwrap();
    leave_a_report(&home);
    let (runner, mut calls) = runner(
        |call| match call.verb {
            Verb::Heartbeat => beat("stop"),
            _ => Answer::Fail(error::unavailable(call.verb, 503)),
        },
        home.clone(),
    );

    serve(runner, CancellationToken::new()).await.unwrap();

    assert!(
        drain(&mut calls)
            .iter()
            .all(|call| call.verb != Verb::Lease)
    );
    assert_eq!(ReportSpool::new(&home).pending().unwrap().len(), 1);
}

#[tokio::test]
async fn a_runner_whose_token_is_refused_does_not_serve() {
    let root = tempfile::tempdir().unwrap();
    let (runner, _calls) = runner(
        |call| Answer::Fail(error::refused(call.verb, 401, None)),
        StorageHome::open(root.path()).unwrap(),
    );

    assert!(
        serve(runner, CancellationToken::new())
            .await
            .unwrap_err()
            .is_unauthorized()
    );
}

#[tokio::test]
async fn running_against_an_unreachable_daemon_fails_at_the_first_beat() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().to_str().unwrap();
    let env = afd_core::env::MapEnv::from_pairs([
        (crate::config::ENV_API_URL, "http://127.0.0.1:1"),
        (crate::config::ENV_RUNNER_TOKEN, "agt_r_token"),
        (crate::config::ENV_STORAGE_HOME, home),
    ]);
    let config = Config::from_env(&env).unwrap();

    let refused = run(
        &config,
        Box::new(FakeEngine::default()),
        Box::new(FakeAgent::new(Behaviour::Answer)),
        probe(),
        CancellationToken::new(),
    )
    .await;

    assert!(refused.unwrap_err().is_retryable());
}
