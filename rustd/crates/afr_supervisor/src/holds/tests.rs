#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot build"
)]

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use afd_core::clock::{FixedClock, UnixMillis};
use afd_core::id::Uuid7;
use afd_core::test_util::trace::Capture;
use afd_core::timing::SANDBOX_HOLD_IDLE_MS;
use afr_sandbox::{Engine, Limits, Sandbox, SandboxRequest};

use super::{BuiltUnder, HoldKey, Holds, Release};
use crate::test_support::FakeEngine;

const START: UnixMillis = UnixMillis::from_millis(1_700_000_000_000);
pub(super) const FLEET_A: &str = "01890a5d-ac96-774b-bcce-b302099a80a1";
pub(super) const FLEET_B: &str = "01890a5d-ac96-774b-bcce-b302099a80a2";
const FLEET_C: &str = "01890a5d-ac96-774b-bcce-b302099a80a3";
const FLEET_D: &str = "01890a5d-ac96-774b-bcce-b302099a80a4";
pub(super) const LEASE: &str = "01890a5d-ac96-774b-bcce-b302099a80b1";
const WORKSPACE: &str = "01890a5d-ac96-774b-bcce-b302099a80c1";
/// The host the filed hold's policy reaches, and one it does not.
const GITHUB: &str = "api.github.com";
const GITLAB: &str = "gitlab.com";
const RELEASED: &str = "sandbox_hold_released";
/// How long a test waits for something that should already have happened.
const PROMPT: Duration = Duration::from_secs(1);

pub(super) fn id(text: &str) -> Uuid7 {
    Uuid7::parse(text).unwrap()
}

pub(super) fn key(fleet: &str) -> HoldKey {
    HoldKey {
        fleet: id(fleet),
        workspace: WORKSPACE.to_owned(),
        limits: Limits::default(),
        policy: BuiltUnder::allowing(&[GITHUB]),
    }
}

/// A registry over a stopped clock, sized for `workers`.
pub(super) fn holds(workers: usize) -> (Holds, FixedClock) {
    let clock = FixedClock::at(START);
    let holds = Holds::start(Arc::new(clock.clone()));
    holds.resize(workers);
    (holds, clock)
}

pub(super) async fn sandbox(engine: &FakeEngine) -> Box<dyn Sandbox> {
    let request = SandboxRequest::new(LEASE, Limits::default());
    engine.prepare(request).await.unwrap()
}

pub(super) async fn park(holds: &Holds, engine: &FakeEngine, fleet: &str) -> Option<UnixMillis> {
    holds
        .park(key(fleet), id(LEASE), sandbox(engine).await)
        .await
}

/// Every release logged, as (fleet, reason), in the order they happened.
pub(super) fn releases(capture: &Capture) -> Vec<(String, String)> {
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

pub(super) fn released(fleet: &str, reason: Release) -> (String, String) {
    (fleet.to_owned(), reason.outcome().as_str().to_owned())
}

pub(super) async fn fleets(holds: &Holds) -> Vec<String> {
    let held = holds.fleets().await;
    held.iter().map(|fleet| fleet.as_str().to_owned()).collect()
}

#[tokio::test]
async fn test_a_parked_sandbox_is_taken_by_its_fleets_next_lease() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (holds, clock) = holds(2);

    let until = park(&holds, &engine, FLEET_A).await;
    clock.advance_millis(30_000);
    let taken = holds.take(&key(FLEET_A)).await.unwrap();

    assert_eq!(
        until,
        Some(START.saturating_add_millis(SANDBOX_HOLD_IDLE_MS))
    );
    assert_eq!(taken.held_ms, 30_000);
    assert!(
        fleets(&holds).await.is_empty(),
        "a taken hold is no longer held"
    );
    let held = capture.only("sandbox_held");
    assert_eq!(held.field("fleet_id"), Some(FLEET_A));
    assert_eq!(held.field("lease_id"), Some(LEASE));
    assert_eq!(engine.destroyed.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn test_mismatch_destroys_the_hold() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (holds, _clock) = holds(4);
    let changes: [fn(&mut HoldKey); 3] = [
        |key| key.workspace = FLEET_D.to_owned(),
        |key| key.limits.memory_bytes /= 2,
        |key| key.policy = BuiltUnder::allowing(&[GITLAB]),
    ];

    for change in changes {
        park(&holds, &engine, FLEET_A).await;
        let mut asked = key(FLEET_A);
        change(&mut asked);
        assert!(holds.take(&asked).await.is_none(), "{asked:?} was served");
    }
    holds.shutdown().await;

    assert_eq!(
        releases(&capture),
        vec![released(FLEET_A, Release::Mismatch); 3]
    );
    assert_eq!(engine.destroyed.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn test_another_fleets_lease_never_takes_or_ends_a_hold() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (holds, _clock) = holds(2);

    park(&holds, &engine, FLEET_A).await;
    let other = holds.take(&key(FLEET_B)).await;

    assert!(other.is_none());
    assert_eq!(fleets(&holds).await, [FLEET_A]);
    let ended = releases(&capture);
    assert!(ended.is_empty(), "{ended:?}");
}

#[tokio::test]
async fn test_expired_hold_is_destroyed() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (holds, clock) = holds(2);

    park(&holds, &engine, FLEET_A).await;
    clock.advance_millis(SANDBOX_HOLD_IDLE_MS - 1);
    assert_eq!(fleets(&holds).await, [FLEET_A], "held until its deadline");
    clock.advance_millis(1);
    let left = fleets(&holds).await;
    assert!(left.is_empty(), "{left:?}");
    assert!(holds.take(&key(FLEET_A)).await.is_none());
    holds.shutdown().await;

    assert_eq!(releases(&capture), [released(FLEET_A, Release::Expired)]);
    assert_eq!(engine.destroyed.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_saturation_releases_holds() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (holds, _clock) = holds(2);
    park(&holds, &engine, FLEET_A).await;
    park(&holds, &engine, FLEET_B).await;

    let _first = holds.occupy(id(FLEET_C));
    assert_eq!(fleets(&holds).await.len(), 2, "one worker is still free");
    let _last = holds.occupy(id(FLEET_D));
    let told = tokio::time::timeout(PROMPT, holds.saturated().notified()).await;

    let left = fleets(&holds).await;
    assert!(left.is_empty(), "{left:?}");
    assert!(told.is_ok(), "the daemon is told at once");
    assert_eq!(
        releases(&capture),
        [
            released(FLEET_A, Release::Saturated),
            released(FLEET_B, Release::Saturated)
        ]
    );
}

#[tokio::test]
async fn test_the_last_free_worker_keeps_the_hold_of_the_fleet_it_serves() {
    let engine = FakeEngine::default();
    let (holds, _clock) = holds(2);
    park(&holds, &engine, FLEET_A).await;
    park(&holds, &engine, FLEET_B).await;

    let _first = holds.occupy(id(FLEET_C));
    let _last = holds.occupy(id(FLEET_A));

    assert_eq!(fleets(&holds).await, [FLEET_A]);
}

#[tokio::test]
async fn test_a_finished_lease_frees_its_worker() {
    let engine = FakeEngine::default();
    let (holds, _clock) = holds(2);
    park(&holds, &engine, FLEET_A).await;

    drop(holds.occupy(id(FLEET_C)));
    let _only = holds.occupy(id(FLEET_D));

    assert_eq!(fleets(&holds).await, [FLEET_A], "one of two workers busy");
}

#[tokio::test]
async fn test_holds_capped_oldest_first() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (holds, _clock) = holds(2);

    for fleet in [FLEET_A, FLEET_B, FLEET_C] {
        park(&holds, &engine, fleet).await;
    }

    assert_eq!(fleets(&holds).await, [FLEET_B, FLEET_C]);
    assert_eq!(releases(&capture), [released(FLEET_A, Release::Capped)]);
}

#[tokio::test]
async fn test_fewer_workers_release_the_oldest_holds() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (holds, _clock) = holds(3);
    for fleet in [FLEET_A, FLEET_B, FLEET_C] {
        park(&holds, &engine, fleet).await;
    }

    holds.resize(1);

    assert_eq!(fleets(&holds).await, [FLEET_C]);
    assert_eq!(
        releases(&capture),
        [
            released(FLEET_A, Release::Capped),
            released(FLEET_B, Release::Capped)
        ]
    );
}

#[tokio::test]
async fn test_a_second_park_for_one_fleet_replaces_the_first() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (holds, _clock) = holds(2);

    park(&holds, &engine, FLEET_A).await;
    park(&holds, &engine, FLEET_A).await;

    assert_eq!(fleets(&holds).await, [FLEET_A]);
    assert_eq!(releases(&capture), [released(FLEET_A, Release::Superseded)]);
}

#[tokio::test]
async fn test_release_ends_one_fleets_hold_for_the_reason_given() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (holds, _clock) = holds(2);
    park(&holds, &engine, FLEET_A).await;
    park(&holds, &engine, FLEET_B).await;

    holds.release(id(FLEET_A), Release::Inactive);

    assert_eq!(fleets(&holds).await, [FLEET_B]);
    assert_eq!(releases(&capture), [released(FLEET_A, Release::Inactive)]);
}

#[tokio::test]
async fn test_a_discarded_sandbox_is_logged_and_destroyed() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (holds, _clock) = holds(2);

    holds
        .discard(id(FLEET_A), sandbox(&engine).await, Release::ThawFailed)
        .await;
    holds.shutdown().await;

    assert_eq!(releases(&capture), [released(FLEET_A, Release::ThawFailed)]);
    assert_eq!(engine.destroyed.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_shutdown_destroys_every_hold_and_waits() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (holds, _clock) = holds(2);
    park(&holds, &engine, FLEET_A).await;
    park(&holds, &engine, FLEET_B).await;

    holds.shutdown().await;

    assert_eq!(engine.destroyed.load(Ordering::SeqCst), 2);
    assert_eq!(
        releases(&capture),
        [
            released(FLEET_A, Release::Shutdown),
            released(FLEET_B, Release::Shutdown)
        ]
    );
}

#[tokio::test]
async fn test_a_park_after_shutdown_destroys_the_sandbox_itself() {
    let engine = FakeEngine::default();
    let (holds, _clock) = holds(2);
    holds.shutdown().await;

    let until = park(&holds, &engine, FLEET_A).await;

    assert_eq!(until, None);
    assert_eq!(engine.destroyed.load(Ordering::SeqCst), 1);
    let left = holds.fleets().await;
    assert!(left.is_empty(), "{left:?}");
}
