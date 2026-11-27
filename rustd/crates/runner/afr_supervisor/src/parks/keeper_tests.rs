//! The task that owns every park: a take nobody waits for, a teardown that
//! fails, a saturation with nothing to give up, what it counts, and how it
//! ends.

#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot build"
)]

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use af_core::clock::FixedClock;
use af_core::test_util::trace::Capture;
use afr_telemetry::labels::SandboxPark;
use afr_telemetry::testing::{Recorded, Tally};
use futures_util::FutureExt as _;

use super::EVENT_DESTROY_FAILED;
use crate::parks::tests::{
    FLEET_A, FLEET_B, RUN, fleets, id, key, park, park_until, parks, released, releases, sandbox,
};
use crate::parks::{Parks, Release, Stowed};
use crate::test_support::{FakeEngine, bounded};

/// How long a test waits for something that should already have happened.
const PROMPT: Duration = Duration::from_secs(1);

/// A run that asked for its fleet's park and stopped waiting before the
/// answer came leaves nobody to take the sandbox: it is destroyed as
/// superseded, never dropped still running.
#[tokio::test]
async fn test_a_take_nobody_waits_for_destroys_the_park() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (parks, _clock) = parks(2);
    park_until(&parks, &engine, FLEET_A).await;

    let abandoned = parks.take(&key(FLEET_A)).now_or_never();
    let left = fleets(&parks).await;
    bounded("the keeper's shutdown", parks.shutdown()).await;

    assert!(abandoned.is_none(), "asked, and gone before the answer");
    assert!(left.is_empty(), "{left:?}");
    assert_eq!(releases(&capture), [released(FLEET_A, Release::Superseded)]);
    assert_eq!(engine.destroyed.load(Ordering::SeqCst), 1);
}

/// A released park whose sandbox will not tear down is logged with its code,
/// so an operator learns what it may have left behind.
#[tokio::test]
async fn test_a_park_that_will_not_tear_down_is_logged() {
    let capture = Capture::install();
    let engine = FakeEngine {
        fail_teardown: true,
        ..FakeEngine::default()
    };
    let (parks, _clock) = parks(2);
    park_until(&parks, &engine, FLEET_A).await;

    parks.release(id(FLEET_A), Release::Inactive);
    bounded("the keeper's shutdown", parks.shutdown()).await;

    let failed = capture.only(EVENT_DESTROY_FAILED);
    assert_eq!(failed.field("fleet_id"), Some(FLEET_A));
    assert!(
        failed
            .field("error_code")
            .is_some_and(|code| !code.is_empty()),
        "{failed:?}"
    );
    assert_eq!(engine.destroyed.load(Ordering::SeqCst), 1, "tried once");
}

/// The last free worker going to the one fleet the runner parks releases
/// nothing, so the heartbeat is not rung early.
#[tokio::test]
async fn test_saturation_that_releases_nothing_does_not_ring() {
    let engine = FakeEngine::default();
    let (parks, _clock) = parks(2);
    park_until(&parks, &engine, FLEET_A).await;

    let _first = parks.occupy(crate::parks::Occupant {
        run: id(FLEET_A),
        fleet: id(FLEET_A),
    });
    let _last = parks.occupy(crate::parks::Occupant {
        run: id(FLEET_B),
        fleet: id(FLEET_B),
    });
    let left = fleets(&parks).await;

    assert_eq!(left, [FLEET_A]);
    let rung = parks.saturated().notified().now_or_never();
    assert!(rung.is_none(), "no beat for nothing released");
}

/// A stopped runner hands out nothing and logs no release for it.
#[tokio::test]
async fn test_after_shutdown_a_take_finds_nothing() {
    let capture = Capture::install();
    let (parks, _clock) = parks(2);
    bounded("the keeper's shutdown", parks.shutdown()).await;

    let taken = bounded("a take after the shutdown", parks.take(&key(FLEET_A))).await;

    assert!(taken.is_none());
    let ended = releases(&capture);
    assert!(ended.is_empty(), "{ended:?}");
}

/// A park is counted at once; a take is not, since whether the sandbox comes
/// back is known only once it resumes and answers, which the run counts.
#[tokio::test]
async fn test_a_park_is_counted_and_a_take_is_not() {
    let engine = FakeEngine::default();
    let (tally, recorded) = Tally::new();
    let clock = Arc::new(FixedClock::at(jiff::Timestamp::UNIX_EPOCH));
    let parks = Parks::recording(clock, tally);
    parks.resize(2);

    park_until(&parks, &engine, FLEET_A).await;
    let taken = bounded("the fleet's park", parks.take(&key(FLEET_A))).await;
    bounded("the keeper's shutdown", parks.shutdown()).await;

    assert!(taken.is_some());
    let counted: Vec<Recorded> = recorded.try_iter().collect();
    assert_eq!(counted, [Recorded::SandboxPark(SandboxPark::Parked)]);
}

/// Once the runner takes no new run the task ends every park, and a sandbox
/// parked after that is handed back to its run rather than parked: the
/// keeper destroys only the park it ended.
#[tokio::test]
async fn test_closed_parks_end_every_park_and_refuse_a_later_park() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (parks, _clock) = parks(2);
    park_until(&parks, &engine, FLEET_A).await;

    parks.close();
    let late = park(&parks, &engine, FLEET_B).await;
    let left = fleets(&parks).await;
    bounded("the keeper's shutdown", parks.shutdown()).await;

    let handed_back = matches!(late, Stowed::Refused(_));
    let destroyed_by_the_keeper = engine.destroyed.load(Ordering::SeqCst);
    if let Stowed::Refused(sandbox) = late {
        sandbox.destroy().await.unwrap();
    }
    assert!(handed_back, "a closed keeper hands the sandbox back");
    assert_eq!(
        destroyed_by_the_keeper, 1,
        "the park it ended, not the late park"
    );
    assert!(left.is_empty(), "{left:?}");
    assert_eq!(
        releases(&capture),
        [
            released(FLEET_A, Release::Shutdown),
            released(FLEET_B, Release::Shutdown)
        ]
    );
}

/// A runner with no workers parks nothing: the sandbox is handed back to its
/// run, its release counted as capped, and the keeper destroys nothing.
#[tokio::test]
async fn test_a_runner_with_no_workers_parks_nothing() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (parks, _clock) = parks(0);

    let parked = park(&parks, &engine, FLEET_A).await;
    bounded("the keeper's shutdown", parks.shutdown()).await;

    let handed_back = matches!(parked, Stowed::Refused(_));
    let destroyed_by_the_keeper = engine.destroyed.load(Ordering::SeqCst);
    if let Stowed::Refused(sandbox) = parked {
        sandbox.destroy().await.unwrap();
    }
    assert!(handed_back, "a keeper with no room hands the sandbox back");
    assert_eq!(destroyed_by_the_keeper, 0, "and destroys nothing itself");
    assert_eq!(releases(&capture), [released(FLEET_A, Release::Capped)]);
}

/// Once every handle is gone the task ends, and what it parked is destroyed,
/// not leaked.
#[tokio::test]
async fn test_dropping_every_handle_destroys_every_park() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (parks, _clock) = parks(2);
    park_until(&parks, &engine, FLEET_A).await;

    drop(parks);
    let torn_down = async {
        while engine.destroyed.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    };

    tokio::time::timeout(PROMPT, torn_down).await.unwrap();
    assert_eq!(releases(&capture), [released(FLEET_A, Release::Shutdown)]);
}

/// Releasing a fleet the runner parks nothing for ends no other park.
#[tokio::test]
async fn test_releasing_a_fleet_not_parked_ends_nothing() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (parks, _clock) = parks(2);
    park_until(&parks, &engine, FLEET_A).await;

    parks.release(id(FLEET_B), Release::Inactive);

    assert_eq!(fleets(&parks).await, [FLEET_A]);
    let ended = releases(&capture);
    assert!(ended.is_empty(), "{ended:?}");
}

/// A clock that jumped back past the bottom of the range between a park and
/// its take reads as a park of no time, not an overflow: the release profile
/// aborts on one, which would end the runner.
#[tokio::test]
async fn test_a_take_across_a_clock_jump_back_reads_as_no_time_parked() {
    let engine = FakeEngine::default();
    let (parks, clock) = parks(2);
    park_until(&parks, &engine, FLEET_A).await;

    clock.set(jiff::Timestamp::MIN);
    let taken = bounded("the fleet's park", parks.take(&key(FLEET_A)))
        .await
        .unwrap();
    bounded("the keeper's shutdown", parks.shutdown()).await;

    assert_eq!(taken.parked, Duration::ZERO);
}

/// A run that parked into a keeper with no room and stopped waiting before
/// the answer came leaves nobody to end the sandbox: the keeper tears it down
/// once, its release counted as capped.
#[tokio::test]
async fn test_a_refused_park_nobody_waits_for_is_torn_down() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (parks, _clock) = parks(0);
    let parked = sandbox(&engine).await;

    let abandoned = parks.park(key(FLEET_A), id(RUN), parked).now_or_never();
    let left = fleets(&parks).await;
    bounded("the keeper's shutdown", parks.shutdown()).await;

    assert!(abandoned.is_none(), "asked, and gone before the answer");
    assert!(left.is_empty(), "{left:?}");
    assert_eq!(releases(&capture), [released(FLEET_A, Release::Capped)]);
    assert_eq!(engine.destroyed.load(Ordering::SeqCst), 1);
}

/// A run that parked into a keeper with room and stopped waiting before
/// the answer came: no report will name the park, so the keeper ends it as
/// superseded at once and tears it down once.
#[tokio::test]
async fn test_a_park_nobody_waits_for_is_superseded() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (parks, _clock) = parks(2);
    let parked = sandbox(&engine).await;

    let abandoned = parks.park(key(FLEET_A), id(RUN), parked).now_or_never();
    let left = fleets(&parks).await;
    bounded("the keeper's shutdown", parks.shutdown()).await;

    assert!(abandoned.is_none(), "asked, and gone before the answer");
    assert!(left.is_empty(), "{left:?}");
    assert_eq!(releases(&capture), [released(FLEET_A, Release::Superseded)]);
    assert_eq!(engine.destroyed.load(Ordering::SeqCst), 1);
}
