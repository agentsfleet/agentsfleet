#![expect(
    clippy::unwrap_used,
    reason = "a test fails loudly on a fixture it cannot build"
)]

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use af_core::clock::{FixedClock, after};
use af_core::id::Uuid7;
use af_core::test_util::trace::Capture;
use af_core::timing::DAY;
use afr_engine::{Engine as _, Limits, ParkMode, Parked, SandboxSpec};
use jiff::Timestamp;

use super::{BuiltUnder, ParkKey, Parks, Release, Stowed};
use crate::test_support::{FakeEngine, bounded};

const START: Timestamp = Timestamp::constant(1_700_000_000, 0);
pub(super) const FLEET_A: &str = "01890a5d-ac96-774b-bcce-b302099a80a1";
pub(super) const FLEET_B: &str = "01890a5d-ac96-774b-bcce-b302099a80a2";
const FLEET_C: &str = "01890a5d-ac96-774b-bcce-b302099a80a3";
const FLEET_D: &str = "01890a5d-ac96-774b-bcce-b302099a80a4";
pub(super) const RUN: &str = "01890a5d-ac96-774b-bcce-b302099a80b1";
const WORKSPACE: &str = "01890a5d-ac96-774b-bcce-b302099a80c1";
/// The host the filed park's policy reaches, and one it does not.
const GITHUB: &str = "api.github.com";
const GITLAB: &str = "gitlab.com";
const RELEASED: &str = "sandbox_park_released";
/// How long a test waits for something that should already have happened.
const PROMPT: Duration = Duration::from_secs(1);

pub(super) fn id(text: &str) -> Uuid7 {
    Uuid7::parse(text).unwrap()
}

pub(super) fn key(fleet: &str) -> ParkKey {
    ParkKey {
        fleet: id(fleet),
        workspace: WORKSPACE.to_owned(),
        limits: Limits::default(),
        policy: BuiltUnder::allowing(&[GITHUB]),
    }
}

/// A registry over a stopped clock, sized for `workers`.
pub(super) fn parks(workers: usize) -> (Parks, FixedClock) {
    let clock = FixedClock::at(START);
    let parks = Parks::start(Arc::new(clock.clone()));
    parks.resize(workers);
    (parks, clock)
}

/// A fresh sandbox, parked, as a run hands it to the keeper.
pub(super) async fn sandbox(engine: &FakeEngine) -> Box<dyn Parked> {
    let run = id(RUN);
    let spec = SandboxSpec::granted_by(engine, &run, Limits::default()).unwrap();
    let sandbox = engine.prepare(spec).await.unwrap();
    sandbox.park(ParkMode::Pause).await.unwrap()
}

/// Parks a fresh sandbox for `fleet`, and answers as the keeper did: a
/// sandbox it refuses is still the caller's to end.
pub(super) async fn park(parks: &Parks, engine: &FakeEngine, fleet: &str) -> Stowed {
    let parked = parks.park(key(fleet), id(RUN), sandbox(engine).await);
    bounded("the keeper's answer to a park", parked).await
}

/// Parks a fresh sandbox for `fleet` into a keeper with room, and answers
/// when the park lapses: none when it was not parked. A refused sandbox is
/// dropped, never destroyed, so a test counting destroys sees the keeper's
/// alone.
pub(super) async fn park_until(
    parks: &Parks,
    engine: &FakeEngine,
    fleet: &str,
) -> Option<Timestamp> {
    match park(parks, engine, fleet).await {
        Stowed::Parked(until) => Some(until),
        Stowed::Refused(_) | Stowed::Gone => None,
    }
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

pub(super) async fn fleets(parks: &Parks) -> Vec<String> {
    let parked = bounded("the keeper's list of parks", parks.fleets()).await;
    parked
        .iter()
        .map(|fleet| fleet.as_str().to_owned())
        .collect()
}

#[tokio::test]
async fn test_a_parked_sandbox_is_taken_by_its_fleets_next_run() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (parks, clock) = parks(2);

    let until = park_until(&parks, &engine, FLEET_A).await;
    clock.advance(Duration::from_secs(30));
    let taken = parks.take(&key(FLEET_A)).await.unwrap();

    assert_eq!(until, Some(after(START, DAY)));
    assert_eq!(taken.parked, Duration::from_secs(30));
    assert!(
        fleets(&parks).await.is_empty(),
        "a taken park is no longer parked"
    );
    let parked = capture.only("sandbox_parked");
    assert_eq!(parked.field("fleet_id"), Some(FLEET_A));
    assert_eq!(parked.field("run_id"), Some(RUN));
    assert_eq!(engine.destroyed.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn test_mismatch_destroys_the_park() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (parks, _clock) = parks(4);
    let changes: [fn(&mut ParkKey); 3] = [
        |key| key.workspace = FLEET_D.to_owned(),
        |key| key.limits.memory_bytes /= 2,
        |key| key.policy = BuiltUnder::allowing(&[GITLAB]),
    ];

    for change in changes {
        park_until(&parks, &engine, FLEET_A).await;
        let mut asked = key(FLEET_A);
        change(&mut asked);
        assert!(parks.take(&asked).await.is_none(), "{asked:?} was served");
    }
    parks.shutdown().await;

    assert_eq!(
        releases(&capture),
        vec![released(FLEET_A, Release::Mismatch); 3]
    );
    assert_eq!(engine.destroyed.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn test_another_fleets_run_never_takes_or_ends_a_park() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (parks, _clock) = parks(2);

    park_until(&parks, &engine, FLEET_A).await;
    let other = parks.take(&key(FLEET_B)).await;

    assert!(other.is_none());
    assert_eq!(fleets(&parks).await, [FLEET_A]);
    let ended = releases(&capture);
    assert!(ended.is_empty(), "{ended:?}");
}

#[tokio::test]
async fn test_expired_park_is_destroyed() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (parks, clock) = parks(2);

    park_until(&parks, &engine, FLEET_A).await;
    clock.advance(DAY.saturating_sub(Duration::from_millis(1)));
    assert_eq!(fleets(&parks).await, [FLEET_A], "parked until its deadline");
    clock.advance(Duration::from_millis(1));
    let left = fleets(&parks).await;
    assert!(left.is_empty(), "{left:?}");
    assert!(parks.take(&key(FLEET_A)).await.is_none());
    parks.shutdown().await;

    assert_eq!(releases(&capture), [released(FLEET_A, Release::Expired)]);
    assert_eq!(engine.destroyed.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn test_saturation_releases_parks() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (parks, _clock) = parks(3);
    park_until(&parks, &engine, FLEET_A).await;
    park_until(&parks, &engine, FLEET_B).await;

    let _first = parks.occupy(super::Occupant {
        run: id(FLEET_C),
        fleet: id(FLEET_C),
    });
    assert_eq!(
        fleets(&parks).await,
        [FLEET_B],
        "one free worker stays reserved"
    );
    let _last = parks.occupy(super::Occupant {
        run: id(FLEET_D),
        fleet: id(FLEET_D),
    });
    let told = tokio::time::timeout(PROMPT, parks.saturated().notified()).await;

    let left = fleets(&parks).await;
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
async fn test_the_last_free_worker_keeps_the_park_of_the_fleet_it_serves() {
    let engine = FakeEngine::default();
    let (parks, _clock) = parks(3);
    park_until(&parks, &engine, FLEET_A).await;
    park_until(&parks, &engine, FLEET_B).await;

    let _first = parks.occupy(super::Occupant {
        run: id(FLEET_A),
        fleet: id(FLEET_A),
    });
    let _last = parks.occupy(super::Occupant {
        run: id(FLEET_C),
        fleet: id(FLEET_C),
    });

    assert_eq!(fleets(&parks).await, [FLEET_A]);
}

#[tokio::test]
async fn test_a_finished_run_frees_its_worker() {
    let engine = FakeEngine::default();
    let (parks, _clock) = parks(3);
    park_until(&parks, &engine, FLEET_A).await;

    drop(parks.occupy(super::Occupant {
        run: id(FLEET_C),
        fleet: id(FLEET_C),
    }));
    let _only = parks.occupy(super::Occupant {
        run: id(FLEET_D),
        fleet: id(FLEET_D),
    });

    assert_eq!(
        fleets(&parks).await,
        [FLEET_A],
        "the completed worker leaves room for the park"
    );
}

#[tokio::test]
async fn test_parks_capped_oldest_first() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (parks, _clock) = parks(3);

    for fleet in [FLEET_A, FLEET_B, FLEET_C] {
        park_until(&parks, &engine, fleet).await;
    }

    assert_eq!(fleets(&parks).await, [FLEET_B, FLEET_C]);
    assert_eq!(releases(&capture), [released(FLEET_A, Release::Capped)]);
}

#[tokio::test]
async fn test_fewer_workers_release_the_oldest_parks() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (parks, _clock) = parks(4);
    for fleet in [FLEET_A, FLEET_B, FLEET_C] {
        park_until(&parks, &engine, fleet).await;
    }

    parks.resize(2);

    assert_eq!(fleets(&parks).await, [FLEET_C]);
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
    let (parks, _clock) = parks(2);

    park_until(&parks, &engine, FLEET_A).await;
    park_until(&parks, &engine, FLEET_A).await;

    assert_eq!(fleets(&parks).await, [FLEET_A]);
    assert_eq!(releases(&capture), [released(FLEET_A, Release::Superseded)]);
}

#[tokio::test]
async fn test_release_ends_one_fleets_park_for_the_reason_given() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let (parks, _clock) = parks(3);
    park_until(&parks, &engine, FLEET_A).await;
    park_until(&parks, &engine, FLEET_B).await;

    parks.release(id(FLEET_A), Release::Inactive);

    assert_eq!(fleets(&parks).await, [FLEET_B]);
    assert_eq!(releases(&capture), [released(FLEET_A, Release::Inactive)]);
}
