#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use afd_core::error_code::RUN_STALE_FENCING_TOKEN;
use afd_core::id::Uuid7;
use afd_core::test_util::trace::Capture;
use afd_wire::lease::LeaseResponse;
use afr_sandbox::{HostProbe, Kvm, Limits};
use bytes::Bytes;
use tokio_util::sync::CancellationToken;

use super::{Runner, run, serve};
use crate::client::{Call, ControlPlane, Verb};
use crate::error;
use crate::holds::Release;
use crate::report_spool::ReportSpool;
use crate::storage_home::StorageHome;
use crate::test_support::{
    Answer, Behaviour, FLEET_ID, FakeAgent, FakeEngine, INTERVAL_MS, LEASE_ID, clock, daemon,
    drain, json, lease, plane,
};

const GRANTED: &str = "01890a5d-ac96-774b-bcce-b302099a8062";
/// The event every release of a held sandbox is logged under.
const RELEASED: &str = "sandbox_hold_released";

fn probe() -> HostProbe {
    HostProbe {
        landlock: true,
        seccomp: true,
        cgroup_controllers: vec!["cpu".to_owned()],
        bubblewrap: true,
        kvm: Kvm::Absent,
        toolbox_filesystem: true,
        workspace_direct_io: None,
    }
}

fn runner(plane: ControlPlane, home: StorageHome) -> Runner {
    Runner {
        plane,
        home,
        engine: Box::new(FakeEngine::default()),
        agent: Box::new(FakeAgent::new(Behaviour::Answer)),
        probe: probe(),
        limits: Limits::default(),
        clock: clock(),
    }
}

/// A daemon that grants one lease, then none, and counts the reports it takes.
fn one_lease_daemon(reports: Arc<AtomicUsize>) -> impl Fn(&Call) -> Answer + Send + Sync + 'static {
    let polled = AtomicUsize::new(0);
    daemon(move |call| match call.verb {
        Verb::Lease if polled.fetch_add(1, Ordering::SeqCst) == 0 => Some(json(&LeaseResponse {
            lease: Some(lease(GRANTED, FLEET_ID, None)),
            retry_after_ms: None,
        })),
        Verb::Report => {
            reports.fetch_add(1, Ordering::SeqCst);
            None
        }
        _other => None,
    })
}

/// A report left behind by a process that died before posting it.
async fn leave_a_report(home: &StorageHome) {
    ReportSpool::new(home)
        .hold(&Uuid7::parse(LEASE_ID).unwrap(), Bytes::from_static(b"{}"))
        .await
        .unwrap();
}

#[tokio::test(start_paused = true)]
async fn a_runner_drains_beats_and_runs_until_shutdown() {
    let root = tempfile::tempdir().unwrap();
    let home = StorageHome::open(root.path()).unwrap();
    leave_a_report(&home).await;
    let reports = Arc::new(AtomicUsize::new(0));
    let (plane, mut calls) = plane(one_lease_daemon(Arc::clone(&reports)));
    let shutdown = CancellationToken::new();
    let serving = tokio::spawn(serve(runner(plane, home.clone()), shutdown.clone()));

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
        verbs.iter().filter(|verb| **verb == Verb::Report).count(),
        2,
        "the left report and the new one"
    );
    assert!(verbs.contains(&Verb::Heartbeat));
    let pending = ReportSpool::new(&home).pending().await.unwrap();
    assert!(pending.is_empty(), "{pending:?}");
}

#[tokio::test(start_paused = true)]
async fn a_stop_on_the_first_beat_serves_nothing_and_a_report_the_daemon_cannot_take_waits() {
    let root = tempfile::tempdir().unwrap();
    let home = StorageHome::open(root.path()).unwrap();
    leave_a_report(&home).await;
    let (plane, mut calls) = plane(daemon(|call| match call.verb {
        Verb::Heartbeat => Some(json(
            &serde_json::json!({"status": "stop", "assigned_policy": null,
            "degraded": false, "degraded_reason": null, "selftest_requested": false,
            "heartbeat_interval_ms": INTERVAL_MS}),
        )),
        Verb::Report => Some(Answer::Fail(error::unavailable(Verb::Report, 503))),
        _other => None,
    }));

    serve(runner(plane, home.clone()), CancellationToken::new())
        .await
        .unwrap();

    assert!(
        drain(&mut calls)
            .iter()
            .all(|call| call.verb != Verb::Lease)
    );
    assert_eq!(ReportSpool::new(&home).pending().await.unwrap().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn a_runner_whose_token_is_refused_stops_with_that_error() {
    let root = tempfile::tempdir().unwrap();
    let (plane, _calls) = plane(|call| Answer::Fail(error::refused(call.verb, 401, None)));

    let stopped = serve(
        runner(plane, StorageHome::open(root.path()).unwrap()),
        CancellationToken::new(),
    )
    .await;

    assert!(stopped.unwrap_err().is_unauthorized());
}

#[tokio::test]
async fn an_unreachable_daemon_is_retried_until_shutdown() {
    let root = tempfile::tempdir().unwrap();
    let home = root.path().to_str().unwrap();
    let env = afd_core::env::MapEnv::from_pairs([
        (crate::config::ENV_API_URL, "http://127.0.0.1:1"),
        (crate::config::ENV_RUNNER_TOKEN, "agt_r_token"),
        (crate::config::ENV_STORAGE_HOME, home),
    ]);
    let (config, home) = crate::boot(&env).unwrap();
    let shutdown = CancellationToken::new();
    let stopping = async {
        tokio::time::sleep(Duration::from_millis(300)).await;
        shutdown.cancel();
    };

    let (served, ()) = tokio::join!(
        run(
            &config,
            home,
            Box::new(FakeEngine::default()),
            Box::new(FakeAgent::new(Behaviour::Answer)),
            probe(),
            shutdown.clone()
        ),
        stopping
    );

    assert!(
        served.is_ok(),
        "a daemon that never answers is waited out, not fatal"
    );
}

/// A runner that stops destroys every sandbox it holds, and each teardown is
/// done before `serve` returns, however long it takes.
#[tokio::test(start_paused = true)]
async fn test_a_stopping_runner_destroys_its_holds() {
    const TEARDOWN: Duration = Duration::from_secs(1);
    let capture = Capture::install();
    let root = tempfile::tempdir().unwrap();
    let home = StorageHome::open(root.path()).unwrap();
    let reports = Arc::new(AtomicUsize::new(0));
    let (plane, _calls) = plane(one_lease_daemon(Arc::clone(&reports)));
    let engine = FakeEngine {
        teardown_takes: TEARDOWN,
        ..FakeEngine::default()
    };
    let destroyed = Arc::clone(&engine.destroyed);
    let composed = Runner {
        engine: Box::new(engine),
        ..runner(plane, home)
    };
    let shutdown = CancellationToken::new();
    let serving = tokio::spawn(serve(composed, shutdown.clone()));

    while reports.load(Ordering::SeqCst) < 1 {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let held_while_serving = destroyed.load(Ordering::SeqCst);
    shutdown.cancel();
    serving.await.unwrap().unwrap();

    assert_eq!(held_while_serving, 0, "the lease's sandbox was held");
    assert_eq!(destroyed.load(Ordering::SeqCst), 1);
    assert_eq!(releases(&capture), [released(Release::Shutdown)]);
}

/// Every release logged, as (fleet, reason), in the order they happened.
fn releases(capture: &Capture) -> Vec<(String, String)> {
    capture
        .events()
        .iter()
        .filter(|event| event.field("event") == Some(RELEASED))
        .map(|event| {
            let field = |name| event.field(name).unwrap().to_owned();
            (field("fleet_id"), field("reason"))
        })
        .collect()
}

/// The granted lease's fleet, released for `reason`, as `releases` reads it.
fn released(reason: Release) -> (String, String) {
    (FLEET_ID.to_owned(), reason.outcome().as_str().to_owned())
}

/// A daemon that grants one lease, answers its report's first post 503, and
/// every post after that as settled without it, counting the posts.
fn superseding_daemon(posts: Arc<AtomicUsize>) -> impl Fn(&Call) -> Answer + Send + Sync + 'static {
    let polled = AtomicUsize::new(0);
    daemon(move |call| match call.verb {
        Verb::Lease if polled.fetch_add(1, Ordering::SeqCst) == 0 => Some(json(&LeaseResponse {
            lease: Some(lease(GRANTED, FLEET_ID, None)),
            retry_after_ms: None,
        })),
        Verb::Report if posts.fetch_add(1, Ordering::SeqCst) == 0 => {
            Some(Answer::Fail(error::unavailable(Verb::Report, 503)))
        }
        Verb::Report => Some(Answer::Fail(error::refused(
            Verb::Report,
            409,
            Some(RUN_STALE_FENCING_TOKEN),
        ))),
        _other => None,
    })
}

/// A report the daemon could not take at once, then answered as settled
/// without it when the drain posted it again: the sandbox its lease parked
/// serves no next lease, and is released as superseded while the runner runs.
#[tokio::test(start_paused = true)]
async fn test_a_drained_superseded_report_releases_what_its_lease_parked() {
    let capture = Capture::install();
    let root = tempfile::tempdir().unwrap();
    let home = StorageHome::open(root.path()).unwrap();
    let posts = Arc::new(AtomicUsize::new(0));
    let (plane, _calls) = plane(superseding_daemon(Arc::clone(&posts)));
    let shutdown = CancellationToken::new();
    let serving = tokio::spawn(serve(runner(plane, home.clone()), shutdown.clone()));

    while posts.load(Ordering::SeqCst) < 2 {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    tokio::time::sleep(Duration::from_secs(1)).await;
    let released_while_serving = releases(&capture);
    shutdown.cancel();
    serving.await.unwrap().unwrap();

    let pending = ReportSpool::new(&home).pending().await.unwrap();
    assert!(pending.is_empty(), "the drain settled it: {pending:?}");
    assert_eq!(released_while_serving, [released(Release::Superseded)]);
}
