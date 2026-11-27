//! The parks a beat releases: one the daemon names inactive is destroyed, one
//! a saturated runner gives up beats at once, and a name that is no fleet id
//! refuses the assignment.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use af_core::id::Uuid7;
use af_core::test_util::trace::Capture;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::tests::{
    FLEET, KEEP_GOING, PARKED_FIELD, RELEASED, STOP, TICK_MS, idle_parks, probe, releasing, sent,
    with_parked,
};
use super::{Assignment, Heartbeat};
use crate::halt::Halt;
use crate::parks::Release;
use crate::test_support::{self, FakeEngine, drain, plane, plane_every};

/// A fleet consuming the worker reserved for cold work.
const OTHER_FLEET: &str = "01890a5d-ac96-774b-bcce-b302099a80a2";
/// A name in `release_parked` that is no fleet id.
const NOT_AN_ID: &str = "not-a-fleet";

/// The daemon names a parked fleet inactive: its park ends as `inactive` and its
/// sandbox is destroyed. A name that is no fleet id is logged with the code
/// for an unreadable identifier, and releases nothing.
#[tokio::test]
async fn test_a_released_park_is_destroyed_as_inactive() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let parks = with_parked(&engine, 2).await;
    let (plane, mut calls) = plane(|_call| releasing(KEEP_GOING, &[FLEET]));
    let probe = probe();

    Heartbeat::new(
        &plane,
        &probe,
        test_support::CAPABILITIES,
        &parks,
        std::path::Path::new("."),
    )
    .beat()
    .await
    .unwrap();
    let left = parks.fleets().await;
    parks.shutdown().await;

    assert!(left.is_empty(), "{left:?}");
    assert_eq!(
        sent(&drain(&mut calls)[0])[PARKED_FIELD],
        serde_json::json!([FLEET])
    );
    let released = capture.only(RELEASED);
    assert_eq!(released.field("fleet_id"), Some(FLEET));
    let inactive = Release::Inactive.outcome().as_str();
    assert_eq!(released.field("reason"), Some(inactive));
    assert_eq!(engine.destroyed.load(Ordering::SeqCst), 1);
}

/// Parks released because the last free worker went to another fleet beat at
/// once, so the daemon stops routing that fleet here before the tick.
#[tokio::test(start_paused = true)]
async fn test_a_saturation_release_beats_at_once() {
    let engine = FakeEngine::default();
    let parks = with_parked(&engine, 2).await;
    let beats = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&beats);
    let (plane, mut calls) =
        plane_every(
            Duration::from_millis(u64::from(TICK_MS)),
            move |_call| match counted.fetch_add(1, Ordering::SeqCst) {
                0 => releasing(KEEP_GOING, &[]),
                _later => releasing(STOP, &[]),
            },
        );
    let probe = probe();
    let (published, mut watching) = watch::channel(Assignment::initial());
    let halt = Halt::new(CancellationToken::new());
    let started = tokio::time::Instant::now();

    let beating = Heartbeat::new(
        &plane,
        &probe,
        test_support::CAPABILITIES,
        &parks,
        std::path::Path::new("."),
    )
    .keep_beating(&published, &halt);
    let saturating = async {
        watching.changed().await.unwrap();
        parks.occupy(crate::parks::Occupant {
            run: Uuid7::parse(OTHER_FLEET).unwrap(),
            fleet: Uuid7::parse(OTHER_FLEET).unwrap(),
        })
    };
    let (_beaten, _busy) = tokio::join!(beating, saturating);

    assert_eq!(
        beats.load(Ordering::SeqCst),
        3,
        "the saturation beat, then the last one"
    );
    assert!(
        started.elapsed() < Duration::from_millis(u64::from(TICK_MS)),
        "the second beat did not wait out the tick"
    );
    let calls = drain(&mut calls);
    assert_eq!(sent(&calls[0])[PARKED_FIELD], serde_json::json!([FLEET]));
    assert_eq!(sent(&calls[1])[PARKED_FIELD], serde_json::json!([]));
}

#[tokio::test]
async fn an_invalid_release_identifier_refuses_the_assignment() {
    let (plane, _calls) = plane(|_| releasing(KEEP_GOING, &[NOT_AN_ID]));
    let probe = probe();
    let parks = idle_parks();
    let mut heartbeat = Heartbeat::new(
        &plane,
        &probe,
        test_support::CAPABILITIES,
        &parks,
        std::path::Path::new("."),
    );
    let failure = heartbeat.beat().await.unwrap_err();
    assert_eq!(
        failure.code(),
        af_core::error_code::INTERNAL_OPERATION_FAILED
    );
    assert!(!heartbeat.last.takes_work(0));
    parks.shutdown().await;
}
