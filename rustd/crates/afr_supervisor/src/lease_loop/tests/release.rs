//! The lease's end in order: the sandbox is killed, the report posts, and
//! only then is its disk released; the lease returns once the release is
//! done, and a kill or a release that fails never holds the report back.

use std::sync::{Arc, Mutex};

use afd_core::test_util::trace::Capture;
use afr_telemetry::labels::SandboxRelease;
use afr_telemetry::testing::{Recorded, Tally, scoped};

use super::super::EVENT_COMPLETED;
use super::super::hold::{EVENT_DESTROY_FAILED, EVENT_KILL_UNSETTLED};
use super::*;
use crate::test_support::{DESTROY, KILL};

/// The step the fake daemon records when the report arrives.
const REPORT: &str = "report";
/// How long the stalled release takes.
const RELEASE_TAKES: Duration = Duration::from_secs(3);

/// A rig whose daemon writes each report into `engine`'s step log, beside
/// the sandbox's kill and destroy.
fn recording(engine: FakeEngine) -> (Rig, Arc<Mutex<Vec<&'static str>>>) {
    let steps = Arc::clone(&engine.steps);
    let reported = Arc::clone(&steps);
    let answers = daemon(move |call| {
        if call.verb == Verb::Report {
            reported.lock().unwrap().push(REPORT);
        }
        None
    });
    (
        Rig::new(answers, engine, FakeAgent::new(Behaviour::Answer)),
        steps,
    )
}

/// The releases the recorder saw, in order.
fn releases(recorded: &std::sync::mpsc::Receiver<Recorded>) -> Vec<SandboxRelease> {
    recorded
        .try_iter()
        .filter_map(|recorded| match recorded {
            Recorded::SandboxRelease(outcome, _elapsed) => Some(outcome),
            _other => None,
        })
        .collect()
}

#[tokio::test(start_paused = true)]
async fn test_report_posts_between_kill_and_release() {
    let (rig, steps) = recording(FakeEngine::default());
    let (tally, recorded) = Tally::new();

    scoped(tally, rig.run(&lease(LEASE_ID, FLEET_ID)))
        .await
        .unwrap();

    assert_eq!(*steps.lock().unwrap(), [KILL, REPORT, DESTROY]);
    assert_eq!(releases(&recorded), [SandboxRelease::Released]);
}

/// A kill that cannot prove the tree empty still lets the report post the
/// run's ending, warns under the lease's name, and claims nothing more.
#[tokio::test(start_paused = true)]
async fn test_unsettled_kill_still_reports() {
    let capture = Capture::install();
    let engine = FakeEngine {
        kill: crate::test_support::Kill::Unsettles,
        ..FakeEngine::default()
    };
    let (mut rig, steps) = recording(engine);

    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

    assert_eq!(reported(&rig.calls())[OUTCOME], PROCESSED);
    assert_eq!(*steps.lock().unwrap(), [KILL, REPORT, DESTROY]);
    let unsettled = capture.only(EVENT_KILL_UNSETTLED);
    assert_eq!(unsettled.field("lease_id"), Some(LEASE_ID));
    assert_eq!(
        unsettled.field("error_code"),
        Some(afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str()),
        "{:?}",
        unsettled.fields
    );
    assert!(
        unsettled
            .field("reason")
            .is_some_and(|reason| reason.contains(crate::test_support::STILL_RUNNING)),
        "{:?}",
        unsettled.fields
    );
}

/// A release that fails after the report changes nothing the report said:
/// it is logged, counted as failed, and its leftovers are the boot sweep's.
#[tokio::test(start_paused = true)]
async fn test_late_release_failure_leaves_work_for_the_sweep() {
    let capture = Capture::install();
    let engine = FakeEngine {
        fail_teardown: true,
        ..FakeEngine::default()
    };
    let (mut rig, steps) = recording(engine);
    let (tally, recorded) = Tally::new();

    scoped(tally, rig.run(&lease(LEASE_ID, FLEET_ID)))
        .await
        .unwrap();

    assert_eq!(reported(&rig.calls())[OUTCOME], PROCESSED);
    assert_eq!(*steps.lock().unwrap(), [KILL, REPORT, DESTROY]);
    assert_eq!(releases(&recorded), [SandboxRelease::Failed]);
    let failed = capture.only(EVENT_DESTROY_FAILED);
    assert_eq!(failed.field("lease_id"), Some(LEASE_ID));
}

/// A slow release no longer holds the report back, but the lease returns
/// only once it is done, so the worker takes no lease while a disk is still
/// held: `report_ms` leaves the release out, and the wall keeps it in.
#[tokio::test(start_paused = true)]
async fn test_a_lease_returns_only_after_its_release() {
    let capture = Capture::install();
    let engine = FakeEngine {
        teardown_takes: RELEASE_TAKES,
        ..FakeEngine::default()
    };
    let destroyed = Arc::clone(&engine.destroyed);
    let (rig, _steps) = recording(engine);
    let started = Instant::now();

    rig.run(&lease(LEASE_ID, FLEET_ID)).await.unwrap();

    assert!(
        started.elapsed() >= RELEASE_TAKES,
        "the lease waited for its release"
    );
    assert_eq!(
        destroyed.load(Ordering::SeqCst),
        1,
        "released before it returned"
    );
    let line = capture.only(EVENT_COMPLETED);
    let report_ms: u64 = line.field("report_ms").unwrap().parse().unwrap();
    assert!(
        u128::from(report_ms) < RELEASE_TAKES.as_millis(),
        "the report did not wait for the disk: {:?}",
        line.fields
    );
}
