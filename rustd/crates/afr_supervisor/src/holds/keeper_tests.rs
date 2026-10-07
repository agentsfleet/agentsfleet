//! The task that owns every hold: a take nobody waits for, a teardown that
//! fails, a saturation with nothing to give up, what it counts, and how it
//! ends.

#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot build"
)]

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use afd_core::clock::{FixedClock, UnixMillis};
use afd_core::test_util::trace::Capture;
use afr_telemetry::labels::SandboxHold;
use afr_telemetry::testing::{Recorded, Tally, scoped};
use futures_util::FutureExt as _;
use tokio::sync::{Notify, mpsc};

use super::{EVENT_DESTROY_FAILED, Keeper};
use crate::holds::tests::{
    FLEET_A, FLEET_B, LEASE, fleets, holds, id, key, park, released, releases, sandbox,
};
use crate::holds::{Holds, Release};
use crate::test_support::FakeEngine;

/// How long a test waits for something that should already have happened.
const PROMPT: Duration = Duration::from_secs(1);

/// A lease that asked for its fleet's hold and stopped waiting before the
/// answer came leaves nobody to take the sandbox: it is destroyed as
/// superseded, never dropped still running.
#[tokio::test]
async fn test_a_take_nobody_waits_for_destroys_the_hold() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (holds, _clock) = holds(2);
    park(&holds, &engine, FLEET_A).await;

    let abandoned = holds.take(&key(FLEET_A)).now_or_never();
    let left = fleets(&holds).await;
    holds.shutdown().await;

    assert!(abandoned.is_none(), "asked, and gone before the answer");
    assert!(left.is_empty(), "{left:?}");
    assert_eq!(releases(&capture), [released(FLEET_A, Release::Superseded)]);
    assert_eq!(engine.destroyed.load(Ordering::SeqCst), 1);
}

/// A released hold whose sandbox will not tear down is logged with its code,
/// so an operator learns what it may have left behind.
#[tokio::test]
async fn test_a_hold_that_will_not_tear_down_is_logged() {
    let capture = Capture::install();
    let engine = FakeEngine {
        fail_teardown: true,
        ..FakeEngine::default()
    };
    let (holds, _clock) = holds(2);
    park(&holds, &engine, FLEET_A).await;

    holds.release(id(FLEET_A), Release::Inactive);
    holds.shutdown().await;

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

/// The last free worker going to the one fleet the runner holds releases
/// nothing, so the heartbeat is not rung early.
#[tokio::test]
async fn test_saturation_that_releases_nothing_does_not_ring() {
    let engine = FakeEngine::default();
    let (holds, _clock) = holds(2);
    park(&holds, &engine, FLEET_A).await;

    let _first = holds.occupy(id(FLEET_B));
    let _last = holds.occupy(id(FLEET_A));
    let left = fleets(&holds).await;

    assert_eq!(left, [FLEET_A]);
    let rung = holds.saturated().notified().now_or_never();
    assert!(rung.is_none(), "no beat for nothing released");
}

/// A stopped runner hands out nothing, and a sandbox discarded after it
/// stopped is still logged and destroyed.
#[tokio::test]
async fn test_after_shutdown_a_take_finds_nothing_and_a_discard_still_destroys() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (holds, _clock) = holds(2);
    holds.shutdown().await;

    let taken = holds.take(&key(FLEET_A)).await;
    let late = sandbox(&engine).await;
    holds.discard(id(FLEET_A), late, Release::ThawFailed).await;

    assert!(taken.is_none());
    assert_eq!(releases(&capture), [released(FLEET_A, Release::ThawFailed)]);
    assert_eq!(engine.destroyed.load(Ordering::SeqCst), 1);
}

/// A park and the take that reuses it are each counted once, in that order.
#[tokio::test]
async fn test_a_park_and_its_reuse_are_counted() {
    let engine = FakeEngine::default();
    let (requests, received) = mpsc::unbounded_channel();
    let saturated = Arc::new(Notify::new());
    let holds = Holds {
        requests,
        saturated: Arc::clone(&saturated),
    };
    let clock = Arc::new(FixedClock::at(UnixMillis::from_millis(0)));
    let (tally, recorded) = Tally::new();
    let keeper = scoped(tally, Keeper::new(clock, saturated).run(received));
    holds.resize(2);
    let leasing = async {
        let parked = sandbox(&engine).await;
        holds.park(key(FLEET_A), id(LEASE), parked).await.unwrap();
        let taken = holds.take(&key(FLEET_A)).await.unwrap();
        holds.shutdown().await;
        taken
    };

    let ((), _taken) = tokio::join!(keeper, leasing);

    let counted: Vec<Recorded> = recorded.try_iter().collect();
    assert_eq!(
        counted,
        [
            Recorded::SandboxHold(SandboxHold::Parked),
            Recorded::SandboxHold(SandboxHold::Reused)
        ]
    );
}

/// A runner with no workers holds nothing: the sandbox is destroyed at once,
/// as capped.
#[tokio::test]
async fn test_a_runner_with_no_workers_holds_nothing() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (holds, _clock) = holds(0);

    let until = park(&holds, &engine, FLEET_A).await;
    holds.shutdown().await;

    assert_eq!(until, None);
    assert_eq!(engine.destroyed.load(Ordering::SeqCst), 1);
    assert_eq!(releases(&capture), [released(FLEET_A, Release::Capped)]);
}

/// Once every handle is gone the task ends, and what it held is destroyed,
/// not leaked.
#[tokio::test]
async fn test_dropping_every_handle_destroys_every_hold() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (holds, _clock) = holds(2);
    park(&holds, &engine, FLEET_A).await;

    drop(holds);
    let torn_down = async {
        while engine.destroyed.load(Ordering::SeqCst) == 0 {
            tokio::task::yield_now().await;
        }
    };

    tokio::time::timeout(PROMPT, torn_down).await.unwrap();
    assert_eq!(releases(&capture), [released(FLEET_A, Release::Shutdown)]);
}

/// Releasing a fleet the runner holds nothing for ends no other hold.
#[tokio::test]
async fn test_releasing_a_fleet_not_held_ends_nothing() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (holds, _clock) = holds(2);
    park(&holds, &engine, FLEET_A).await;

    holds.release(id(FLEET_B), Release::Inactive);

    assert_eq!(fleets(&holds).await, [FLEET_A]);
    let ended = releases(&capture);
    assert!(ended.is_empty(), "{ended:?}");
}
