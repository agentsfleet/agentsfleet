//! A stop never waits on a daemon that does not answer: a beat in flight is
//! abandoned for the last one, and the last one is given up on once
//! [`LAST_BEAT_TIMEOUT`] passes.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use afd_core::test_util::trace::Capture;
use tokio::sync::watch;
use tokio::time::{Instant, timeout};
use tokio_util::sync::CancellationToken;

use super::closing_tests::closing;
use super::tests::{KEEP_GOING, probe, releasing};
use super::{Assignment, EVENT_LAST_FAILED, Heartbeat, LAST_BEAT_TIMEOUT};
use crate::client::{ControlPlane, Verb};
use crate::halt::Halt;
use crate::holds::Holds;
use crate::test_support::{Answer, clock, drain, plane};

/// How long the runner serves before it is told to stop.
const STOP_AFTER: Duration = Duration::from_secs(1);

/// Past every wait a stop may spend: serving, then one bounded last beat. A
/// runner still beating here waited on a daemon that never answered.
const BOUND: Duration = STOP_AFTER.saturating_add(LAST_BEAT_TIMEOUT);

/// Serves over `plane` until `shutdown` is cancelled `STOP_AFTER` in, and
/// answers whether the heartbeat ended inside [`BOUND`].
async fn stops_in_bound(plane: &ControlPlane, shutdown: CancellationToken) -> bool {
    let probe = probe();
    let holds = Holds::start(clock());
    let (published, _watching) = watch::channel(Assignment::initial());
    let halt = Halt::new(shutdown.clone());

    let beating = Heartbeat::new(plane, &probe, &holds).keep_beating(&published, &halt);
    let stopping = async {
        tokio::time::sleep(STOP_AFTER).await;
        shutdown.cancel();
    };
    let ended = timeout(BOUND, async { tokio::join!(beating, stopping) }).await;
    holds.shutdown().await;
    ended.is_ok()
}

/// The daemon stops answering mid-beat and the runner is told to stop: the
/// beat in flight is abandoned and the last beat goes at once.
#[tokio::test(start_paused = true)]
async fn test_a_stop_abandons_a_beat_in_flight_for_the_last_one() {
    let beats = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&beats);
    let (plane, mut calls) = plane(move |_call| match counted.fetch_add(1, Ordering::SeqCst) {
        0 => Answer::Stall,
        _last => releasing(KEEP_GOING, &[]),
    });
    let started = Instant::now();

    let ended = stops_in_bound(&plane, CancellationToken::new()).await;

    assert!(ended, "the stop waited on the beat in flight");
    assert_eq!(
        started.elapsed(),
        STOP_AFTER,
        "the last beat went at the stop"
    );
    assert_eq!(
        closing(&drain(&mut calls)),
        [false, true],
        "the abandoned beat, then the last one"
    );
}

/// A last beat the daemon never answers is given up on once the bound
/// passes, and logged as the heartbeat failure it is.
#[tokio::test(start_paused = true)]
async fn test_an_unanswered_last_beat_is_given_up_after_its_bound() {
    let capture = Capture::install();
    let (plane, mut calls) = plane(|_call| Answer::Stall);
    let shutdown = CancellationToken::new();
    shutdown.cancel();
    let started = Instant::now();

    let ended = stops_in_bound(&plane, shutdown).await;

    assert!(ended, "the stop waited on an unanswered last beat");
    assert_eq!(started.elapsed(), LAST_BEAT_TIMEOUT);
    assert_eq!(drain(&mut calls).len(), 1, "the last beat alone");
    let failed = capture.only(EVENT_LAST_FAILED);
    let code = Verb::Heartbeat.code().as_str();
    assert_eq!(failed.field("error_code"), Some(code));
}
