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
    assert!(holds.park(key, lease, sandbox).await.is_some(), "parked");
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
