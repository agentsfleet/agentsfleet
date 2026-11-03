#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::fs;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use afd_core::id::Uuid7;
use afd_core::test_util::trace::Capture;
use afd_wire::report::ReportResponse;
use afr_sandbox::{Engine as _, Limits, SandboxRequest};
use bytes::Bytes;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use super::Drainer;
use crate::client::{Call, Verb};
use crate::error;
use crate::halt::Halt;
use crate::holds::{BuiltUnder, HoldKey, Holds, Release};
use crate::report_spool::ReportSpool;
use crate::storage_home::StorageHome;
use crate::test_support::{Answer, FLEET_ID, FakeEngine, LEASE_ID, clock, json, plane};

/// A spooled report's bytes; the drainer never reads them.
const EMPTY_REPORT: &[u8] = b"{}";

const SECOND_LEASE: &str = "01890a5d-ac96-774b-bcce-b302099a8063";

/// A daemon that answers the first `busy` report posts 503, then takes them.
fn busy_then_ok(busy: usize, posts: Arc<AtomicUsize>) -> impl Fn(&Call) -> Answer + Send + Sync {
    move |_call| {
        if posts.fetch_add(1, Ordering::SeqCst) < busy {
            Answer::Fail(error::unavailable(Verb::Report, 503))
        } else {
            json(&ReportResponse { ok: true })
        }
    }
}

async fn until_empty(spool: &ReportSpool) {
    while !spool.pending().await.unwrap().is_empty() {
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test(start_paused = true)]
async fn a_held_report_is_posted_again_until_taken_and_a_new_one_wakes_the_drain() {
    let root = tempfile::tempdir().unwrap();
    let home = StorageHome::open(root.path()).unwrap();
    let spool = ReportSpool::new(&home);
    spool
        .hold(
            &Uuid7::parse(LEASE_ID).unwrap(),
            Bytes::from_static(EMPTY_REPORT),
        )
        .await
        .unwrap();
    let posts = Arc::new(AtomicUsize::new(0));
    let (plane, _calls) = plane(busy_then_ok(3, Arc::clone(&posts)));
    let shutdown = CancellationToken::new();
    let halt = Halt::new(shutdown.clone());
    let held = Notify::new();
    let holds = Holds::start(clock());
    let drainer = Drainer {
        spool: &spool,
        plane: &plane,
        halt: &halt,
        held: &held,
        holds: &holds,
    };

    let ((), ()) = tokio::join!(drainer.run(), async {
        until_empty(&spool).await;
        assert_eq!(posts.load(Ordering::SeqCst), 4, "three blips, then taken");
        tokio::time::sleep(Duration::from_secs(600)).await;
        assert_eq!(
            posts.load(Ordering::SeqCst),
            4,
            "an empty spool is not polled"
        );
        spool
            .hold(
                &Uuid7::parse(SECOND_LEASE).unwrap(),
                Bytes::from_static(EMPTY_REPORT),
            )
            .await
            .unwrap();
        held.notify_one();
        until_empty(&spool).await;
        shutdown.cancel();
    });

    assert_eq!(posts.load(Ordering::SeqCst), 5);
}

#[tokio::test(start_paused = true)]
async fn a_refused_token_stops_the_runner_from_the_drain() {
    let root = tempfile::tempdir().unwrap();
    let home = StorageHome::open(root.path()).unwrap();
    let spool = ReportSpool::new(&home);
    spool
        .hold(
            &Uuid7::parse(LEASE_ID).unwrap(),
            Bytes::from_static(EMPTY_REPORT),
        )
        .await
        .unwrap();
    let (plane, _calls) = plane(|_call| Answer::Fail(error::refused(Verb::Report, 401, None)));
    let halt = Halt::new(CancellationToken::new());
    let held = Notify::new();
    let holds = Holds::start(clock());

    Drainer {
        spool: &spool,
        plane: &plane,
        halt: &halt,
        held: &held,
        holds: &holds,
    }
    .run()
    .await;

    assert!(halt.token_refused());
    assert_eq!(
        spool.pending().await.unwrap().len(),
        1,
        "kept for a runner with a good token"
    );
}

#[tokio::test(start_paused = true)]
async fn a_spool_that_will_not_read_is_retried_rather_than_abandoned() {
    let root = tempfile::tempdir().unwrap();
    let home = StorageHome::open(root.path()).unwrap();
    let spool = ReportSpool::new(&home);
    spool
        .hold(
            &Uuid7::parse(LEASE_ID).unwrap(),
            Bytes::from_static(EMPTY_REPORT),
        )
        .await
        .unwrap();
    let stuck = home.spool().join(LEASE_ID).with_extension("json");
    fs::remove_file(&stuck).unwrap();
    fs::create_dir(&stuck).unwrap();
    let (plane, mut calls) = plane(|_call| json(&ReportResponse { ok: true }));
    let shutdown = CancellationToken::new();
    let halt = Halt::new(shutdown.clone());
    let held = Notify::new();
    let holds = Holds::start(clock());
    let drainer = Drainer {
        spool: &spool,
        plane: &plane,
        halt: &halt,
        held: &held,
        holds: &holds,
    };

    let ((), ()) = tokio::join!(drainer.run(), async {
        tokio::time::sleep(Duration::from_secs(5)).await;
        fs::remove_dir(&stuck).unwrap();
        spool
            .hold(
                &Uuid7::parse(LEASE_ID).unwrap(),
                Bytes::from_static(EMPTY_REPORT),
            )
            .await
            .unwrap();
        until_empty(&spool).await;
        shutdown.cancel();
    });

    let posted = crate::test_support::drain(&mut calls).len();
    assert_eq!(posted, 1, "posted once the entry read");
}

/// A registry with room for two, holding a sandbox `engine` built that the
/// lease [`LEASE_ID`] parked for [`FLEET_ID`].
async fn parked(engine: &FakeEngine) -> Holds {
    let holds = Holds::start(clock());
    holds.resize(2);
    let request = SandboxRequest::new(LEASE_ID, Limits::default());
    let key = HoldKey {
        fleet: Uuid7::parse(FLEET_ID).unwrap(),
        workspace: FLEET_ID.to_owned(),
        limits: Limits::default(),
        policy: BuiltUnder::allowing(&[]),
    };
    let sandbox = engine.prepare(request).await.unwrap();
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    assert!(
        matches!(
            holds.park(key, lease, sandbox).await,
            crate::holds::Parked::Held(_)
        ),
        "parked"
    );
    holds
}

/// A spooled report the daemon will never take is set aside by the drain,
/// and the sandbox its lease parked is destroyed: the daemon never recorded
/// the run it carries.
#[tokio::test(start_paused = true)]
async fn test_a_drained_rejected_report_releases_what_its_lease_parked() {
    const RELEASED: &str = "sandbox_hold_released";
    let capture = Capture::install();
    let root = tempfile::tempdir().unwrap();
    let home = StorageHome::open(root.path()).unwrap();
    let spool = ReportSpool::new(&home);
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    spool
        .hold(&lease, Bytes::from_static(EMPTY_REPORT))
        .await
        .unwrap();
    let (plane, _calls) = plane(|_call| Answer::Fail(error::refused(Verb::Report, 400, None)));
    let shutdown = CancellationToken::new();
    let halt = Halt::new(shutdown.clone());
    let held = Notify::new();
    let engine = FakeEngine::default();
    let holds = parked(&engine).await;
    let drainer = Drainer {
        spool: &spool,
        plane: &plane,
        halt: &halt,
        held: &held,
        holds: &holds,
    };

    let ((), ()) = tokio::join!(drainer.run(), async {
        until_empty(&spool).await;
        shutdown.cancel();
    });
    let left = holds.fleets().await;
    holds.shutdown().await;

    assert!(left.is_empty(), "{left:?}");
    let released = capture.only(RELEASED);
    let superseded = Release::Superseded.outcome().as_str();
    assert_eq!(released.field("reason"), Some(superseded));
    assert_eq!(engine.destroyed.load(Ordering::SeqCst), 1);
}

/// A one-post report a lease left held, its memory and segments beside the
/// rest of the body.
const ONE_POST_REPORT: &str = r#"{"lease_id":"x","memory":[{"key":"k","content":"v","category":"core"}],"telemetry":{"wall_ms":5,"segments":{"settle_ms":2}}}"#;

/// That report as the older replica reads it: no memory, no segments.
fn plain_report() -> serde_json::Value {
    serde_json::json!({"lease_id": "x", "telemetry": {"wall_ms": 5}})
}

/// The refusal an older replica gives a body shaped past what it reads.
fn shape_refused() -> Answer {
    let shape = Some(afd_core::error_code::INVALID_REQUEST);
    Answer::Fail(error::refused(Verb::Report, 400, shape))
}

/// Drains a spool holding [`ONE_POST_REPORT`] against a daemon answering its
/// `n`th report post with `answer(n)`, until the spool empties or ten virtual
/// minutes pass. Returns the bodies posted and the files left in the spool.
async fn drain_one_post<F>(answer: F) -> (Vec<serde_json::Value>, Vec<std::path::PathBuf>)
where
    F: Fn(usize) -> Answer + Send + Sync + 'static,
{
    let root = tempfile::tempdir().unwrap();
    let home = StorageHome::open(root.path()).unwrap();
    let spool = ReportSpool::new(&home);
    spool
        .hold(
            &Uuid7::parse(LEASE_ID).unwrap(),
            Bytes::from_static(ONE_POST_REPORT.as_bytes()),
        )
        .await
        .unwrap();
    let posts = AtomicUsize::new(0);
    let (plane, mut calls) = plane(move |_call| answer(posts.fetch_add(1, Ordering::SeqCst)));
    let shutdown = CancellationToken::new();
    let halt = Halt::new(shutdown.clone());
    let held = Notify::new();
    let holds = Holds::start(clock());
    let drainer = Drainer {
        spool: &spool,
        plane: &plane,
        halt: &halt,
        held: &held,
        holds: &holds,
    };

    let ((), _emptied) = tokio::join!(drainer.run(), async {
        let emptied = tokio::time::timeout(Duration::from_secs(600), until_empty(&spool)).await;
        shutdown.cancel();
        emptied
    });

    let bodies = crate::test_support::drain(&mut calls)
        .iter()
        .map(|call| serde_json::from_slice(call.body.as_ref().unwrap()).unwrap())
        .collect();
    let left = fs::read_dir(home.spool())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_file())
        .collect();
    (bodies, left)
}

/// A report the drain posts to an older replica, which refuses its shape, is
/// posted again without the run's memory and segments, and nothing is left in
/// the spool: the answer is kept though the memory is not.
#[tokio::test(start_paused = true)]
async fn test_drain_reposts_a_shape_refused_report_without_its_memory() {
    let capture = Capture::install();

    let drained = drain_one_post(|post| {
        if post == 0 {
            shape_refused()
        } else {
            json(&ReportResponse { ok: true })
        }
    })
    .await;

    let one_post: serde_json::Value = serde_json::from_str(ONE_POST_REPORT).unwrap();
    assert_eq!(drained, (vec![one_post, plain_report()], Vec::new()));
    let dropped = capture.only("report_memory_dropped");
    assert_eq!(
        (dropped.field("lease_id"), dropped.field("cause_error_code")),
        (Some(LEASE_ID), Some("UZ-REQ-001"))
    );
}

/// A reshaped report the daemon keeps, a 503 mid-deploy, stays held and is
/// posted again on the drain's backoff rather than waiting on an unrelated
/// wake; the spool empties once the daemon takes it.
#[tokio::test(start_paused = true)]
async fn test_a_reshaped_report_the_daemon_keeps_is_retried() {
    let drained = drain_one_post(|post| match post {
        0 => shape_refused(),
        1 => Answer::Fail(error::unavailable(Verb::Report, 503)),
        _ => json(&ReportResponse { ok: true }),
    })
    .await;

    let one_post: serde_json::Value = serde_json::from_str(ONE_POST_REPORT).unwrap();
    assert_eq!(
        drained,
        (vec![one_post, plain_report(), plain_report()], Vec::new())
    );
}

/// A refusal of any other shape is quarantined as before, never reshaped.
#[test]
fn test_only_a_shape_refused_one_post_body_is_reshaped() {
    let shape = error::refused(
        Verb::Report,
        400,
        Some(afd_core::error_code::INVALID_REQUEST),
    );
    let other = error::refused(Verb::Report, 400, None);
    let plain = br#"{"lease_id":"x","telemetry":{"wall_ms":5}}"#;
    assert_eq!(
        (
            crate::report_spool::reshaped(&other, ONE_POST_REPORT.as_bytes()),
            crate::report_spool::reshaped(&shape, plain),
            crate::report_spool::reshaped(&shape, b"not json"),
        ),
        (None, None, None)
    );
}
