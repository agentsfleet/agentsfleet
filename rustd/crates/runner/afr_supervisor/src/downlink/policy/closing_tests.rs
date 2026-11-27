//! A runner that takes no new run ends every park and tells the daemon at
//! once, whether running alone stopped or the runner is shutting down.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use af_core::test_util::trace::Capture;
use afr_engine::HostProbe;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::tests::{
    FLEET, KEEP_GOING, PARKED_FIELD, RELEASED, STOP, TICK_MS, park, probe, releasing, sent,
    with_parked,
};
use super::{Assignment, Heartbeat};
use crate::client::{Call, ControlPlane, Verb};
use crate::error;
use crate::halt::Halt;
use crate::parks::{Parks, Release, Stowed};
use crate::test_support::{Answer, FakeEngine, drain, plane, plane_every};

/// The beat the daemon answers `stop`, once running stopped two beats in.
const STOPPED_AT_BEAT: usize = 3;
/// The beat's field saying the runner's parks list is final.
pub(super) const CLOSING_FIELD: &str = "closing";

/// The field every logged event names itself under.
pub(super) const EVENT_FIELD: &str = "event";

/// Every park the beat ended, by the reason it was logged under.
fn reasons(capture: &Capture) -> Vec<String> {
    capture
        .events()
        .iter()
        .filter(|event| event.field(EVENT_FIELD) == Some(RELEASED))
        .map(|event| event.field("reason").unwrap().to_owned())
        .collect()
}

/// The daemon refusing this runner's token.
pub(super) fn token_refusal() -> crate::Error {
    error::refused(Verb::Heartbeat, 401, None)
}

/// Every park has ended and none can start: the registry lists nothing, and a
/// sandbox parked now is handed back to its run rather than parked, the
/// keeper destroying only the park it ended.
async fn assert_closed(parks: &Parks, engine: &FakeEngine) {
    let left = parks.fleets().await;
    let parked = park(parks, engine).await;
    parks.shutdown().await;

    let handed_back = matches!(parked, Stowed::Refused(_));
    let destroyed_by_the_keeper = engine.destroyed.load(Ordering::SeqCst);
    if let Stowed::Refused(sandbox) = parked {
        sandbox.destroy().await.unwrap();
    }
    none_left(&left);
    assert!(handed_back, "a park after the close is handed back");
    assert_eq!(
        destroyed_by_the_keeper, 1,
        "the park alone, not the late park"
    );
}

/// Whether each beat said its list was final, in the order the daemon
/// received them.
pub(super) fn closing(calls: &[Call]) -> Vec<bool> {
    calls
        .iter()
        .map(|call| sent(call)[CLOSING_FIELD].as_bool().unwrap())
        .collect()
}

/// A heartbeat over `plane`, reporting `parks`, as the runner beats.
pub(super) fn heartbeat<'a>(
    plane: &'a ControlPlane,
    probe: &'a HostProbe,
    parks: &'a Parks,
) -> Heartbeat<'a> {
    Heartbeat::new(
        plane,
        probe,
        crate::test_support::CAPABILITIES,
        parks,
        std::path::Path::new("."),
    )
}

/// The parks each beat listed, in the order the daemon received them.
fn listed(calls: &[Call]) -> Vec<serde_json::Value> {
    calls
        .iter()
        .map(|call| sent(call)[PARKED_FIELD].clone())
        .collect()
}

/// Leasing stops while the runner serves on: every park ends, since no run
/// could take one, and the next beat goes at once listing none, rather than
/// at the tick, so the daemon stops routing those fleets here.
#[tokio::test(start_paused = true)]
async fn test_leasing_stopped_ends_every_park_and_beats_at_once() {
    let capture = Capture::install();
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

    let beating = heartbeat(&plane, &probe, &parks).keep_beating(&published, &halt);
    let stopping = async {
        watching.changed().await.unwrap();
        halt.taking_runs().cancel();
    };
    let ((), ()) = tokio::join!(beating, stopping);
    parks.shutdown().await;

    assert!(
        started.elapsed() < Duration::from_millis(u64::from(TICK_MS)),
        "the beat after running stopped did not wait out the tick"
    );
    let calls = drain(&mut calls);
    let listed = listed(&calls);
    assert_eq!(listed[0], serde_json::json!([FLEET]));
    assert_eq!(listed[1], serde_json::json!([]), "{listed:?}");
    assert_eq!(
        closing(&calls)[..2],
        [false, true],
        "the list is final once closed"
    );
    let shutdown = Release::Shutdown.outcome().as_str();
    assert_eq!(reasons(&capture), [shutdown]);
    assert_eq!(engine.destroyed.load(Ordering::SeqCst), 1);
}

/// A runner shutting down ends every park and says so in one last beat, so
/// the daemon routes those fleets elsewhere now rather than once it finds
/// this runner gone.
#[tokio::test(start_paused = true)]
async fn test_a_shutdown_ends_every_park_and_says_so_in_a_last_beat() {
    let capture = Capture::install();
    let engine = FakeEngine::default();
    let parks = with_parked(&engine, 2).await;
    let (plane, mut calls) = plane(|_call| releasing(KEEP_GOING, &[]));
    let probe = probe();
    let (published, mut watching) = watch::channel(Assignment::initial());
    let shutdown = CancellationToken::new();
    let halt = Halt::new(shutdown.clone());

    let beating = heartbeat(&plane, &probe, &parks).keep_beating(&published, &halt);
    let stopping = async {
        watching.changed().await.unwrap();
        shutdown.cancel();
    };
    let ((), ()) = tokio::join!(beating, stopping);
    let left = parks.fleets().await;
    parks.shutdown().await;

    none_left(&left);
    let calls = drain(&mut calls);
    assert_eq!(
        listed(&calls),
        [serde_json::json!([FLEET]), serde_json::json!([])],
        "the beat, then the last one listing nothing"
    );
    assert_eq!(
        closing(&calls),
        [false, true],
        "the last beat's list is final"
    );
    let ended = Release::Shutdown.outcome().as_str();
    assert_eq!(reasons(&capture), [ended]);
    assert_eq!(engine.destroyed.load(Ordering::SeqCst), 1);
}

/// A beat refused for the token ends every park at once, and sends no last
/// beat: the runner stops, so no run could take a park, and no call can
/// succeed to say so.
#[tokio::test(start_paused = true)]
async fn test_a_refused_runner_ends_its_parks_and_sends_no_last_beat() {
    let engine = FakeEngine::default();
    let parks = with_parked(&engine, 2).await;
    let (plane, mut calls) = plane(|_call| Answer::Fail(token_refusal()));
    let probe = probe();
    let (published, _watching) = watch::channel(Assignment::initial());
    let halt = Halt::new(CancellationToken::new());

    heartbeat(&plane, &probe, &parks)
        .keep_beating(&published, &halt)
        .await;

    assert!(halt.token_refused());
    assert_eq!(drain(&mut calls).len(), 1, "the refused beat alone");
    assert_closed(&parks, &engine).await;
}

/// Leasing stops once, so it brings one beat forward and no more: every beat
/// after it waits out the interval again while the daemon keeps answering
/// keep-going.
#[tokio::test(start_paused = true)]
async fn test_leasing_stopped_brings_one_beat_forward_and_no_more() {
    let engine = FakeEngine::default();
    let parks = with_parked(&engine, 2).await;
    let stamps = Arc::new(Mutex::new(Vec::new()));
    let stamped = Arc::clone(&stamps);
    let (plane, mut calls) = plane_every(Duration::from_millis(u64::from(TICK_MS)), move |_call| {
        let mut beats = stamped.lock().unwrap();
        beats.push(tokio::time::Instant::now());
        let status = if beats.len() < STOPPED_AT_BEAT {
            KEEP_GOING
        } else {
            STOP
        };
        releasing(status, &[])
    });
    let probe = probe();
    let (published, mut watching) = watch::channel(Assignment::initial());
    let halt = Halt::new(CancellationToken::new());
    let started = tokio::time::Instant::now();

    let beating = heartbeat(&plane, &probe, &parks).keep_beating(&published, &halt);
    let stopping = async {
        watching.changed().await.unwrap();
        halt.taking_runs().cancel();
    };
    let ((), ()) = tokio::join!(beating, stopping);
    parks.shutdown().await;

    let tick = Duration::from_millis(u64::from(TICK_MS));
    let beats = stamps.lock().unwrap().clone();
    let early = beats.iter().filter(|at| **at - started < tick).count();
    assert_eq!(early, 2, "the first beat and the one brought forward");
    assert!(beats[2] - started >= tick, "the third waited out the tick");
    assert_eq!(
        closing(&drain(&mut calls)),
        [false, true, true, true],
        "every beat after the close, the last one too, says its list is final"
    );
}

/// A token refused on another call stops serving while the beat waits: the
/// last beat ends every park and sends nothing, since no call can succeed.
#[tokio::test(start_paused = true)]
async fn test_a_token_refused_elsewhere_ends_every_park_without_a_last_beat() {
    let engine = FakeEngine::default();
    let parks = with_parked(&engine, 2).await;
    let (plane, mut calls) = plane(|_call| releasing(KEEP_GOING, &[]));
    let probe = probe();
    let (published, mut watching) = watch::channel(Assignment::initial());
    let halt = Halt::new(CancellationToken::new());

    let beating = heartbeat(&plane, &probe, &parks).keep_beating(&published, &halt);
    let refusing = async {
        watching.changed().await.unwrap();
        assert!(halt.stops_on(&token_refusal()), "a refused token stops");
    };
    let ((), ()) = tokio::join!(beating, refusing);

    assert_eq!(drain(&mut calls).len(), 1, "the beat alone, no last one");
    assert_closed(&parks, &engine).await;
}

/// Every park ended: the keeper lists no fleet.
pub(super) fn none_left(left: &[af_core::id::Uuid7]) {
    assert!(left.is_empty(), "still parked {left:?}");
}
