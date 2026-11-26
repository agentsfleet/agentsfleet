//! The last beat a runner sends as it shuts down: one the daemon cannot take
//! is logged once and not retried, and one refused for its token is no
//! failure of the beat's.

#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use af_core::test_util::trace::Capture;
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

use super::closing_tests::{EVENT_FIELD, heartbeat, none_left, token_refusal};
use super::tests::{KEEP_GOING, holding, probe, releasing};
use super::{Assignment, EVENT_LAST_FAILED};
use crate::client::Verb;
use crate::error;
use crate::halt::Halt;
use crate::test_support::{Answer, FakeEngine, drain, plane};

/// Beats once, then shuts down with the daemon answering the last beat with
/// `failure`, after exactly that beat and the last one: answers which holds
/// are left.
async fn last_beat_answered(failure: fn() -> crate::Error) -> Vec<af_core::id::Uuid7> {
    let engine = FakeEngine::default();
    let holds = holding(&engine, 2).await;
    let beats = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&beats);
    let (plane, mut calls) = plane(move |_call| match counted.fetch_add(1, Ordering::SeqCst) {
        0 => releasing(KEEP_GOING, &[]),
        _later => Answer::Fail(failure()),
    });
    let probe = probe();
    let (published, mut watching) = watch::channel(Assignment::initial());
    let shutdown = CancellationToken::new();
    let halt = Halt::new(shutdown.clone());

    let beating = heartbeat(&plane, &probe, &holds).keep_beating(&published, &halt);
    let stopping = async {
        watching.changed().await.unwrap();
        shutdown.cancel();
    };
    let ((), ()) = tokio::join!(beating, stopping);
    let left = holds.fleets().await;
    holds.shutdown().await;
    assert_eq!(drain(&mut calls).len(), 2, "the beat, then one last beat");
    left
}

/// The daemon unable to take a beat.
fn unavailable() -> crate::Error {
    error::unavailable(Verb::Heartbeat, 503)
}

/// A last beat the daemon cannot take is logged under its code and not
/// retried: the runner stops either way, and its holds lapse at the daemon.
#[tokio::test(start_paused = true)]
async fn test_a_failed_last_beat_is_logged_once_and_not_retried() {
    let capture = Capture::install();

    let left = last_beat_answered(unavailable).await;

    let failed = capture.only(EVENT_LAST_FAILED);
    assert_eq!(
        failed.field("error_code"),
        Some(unavailable().code().as_str())
    );
    none_left(&left);
}

/// A last beat refused for its token is not logged as failed: with the token
/// gone no call could have succeeded, which is no failure of the beat's.
#[tokio::test(start_paused = true)]
async fn test_a_last_beat_refused_for_its_token_is_not_logged_as_failed() {
    let capture = Capture::install();

    let left = last_beat_answered(token_refusal).await;

    let logged = capture.events();
    assert!(
        !logged
            .iter()
            .any(|event| event.field(EVENT_FIELD) == Some(EVENT_LAST_FAILED)),
        "a refused token logs no failed last beat"
    );
    none_left(&left);
}
