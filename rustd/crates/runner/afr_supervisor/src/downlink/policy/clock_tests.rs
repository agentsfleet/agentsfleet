//! The heartbeat loop beats when the runner's one clock rings it, and at no other
//! time: a pushed policy does not move the next beat, and a round that falls while a
//! beat is in flight is kept for exactly one more beat.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::watch;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::tests::{KEEP_GOING, answer, idle_holds, policy, probe};
use super::{Assignment, Heartbeat};
use crate::halt::Halt;
use crate::test_support::{Answer, plane};

/// Runs the heartbeat loop over a plane that answers each beat with `answering`, until
/// `until` has passed, then stops; answers when each beat was sent, from the start.
async fn beats_until(
    until: Duration,
    answering: impl Fn(usize) -> Answer + Send + Sync + 'static,
    during: impl AsyncFnOnce(&crate::test_support::Calls),
) -> Vec<Duration> {
    let started = Instant::now();
    let stamps = Arc::new(Mutex::new(Vec::new()));
    let stamped = Arc::clone(&stamps);
    let (plane, calls) = plane(move |_call| {
        let mut beats = stamped
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        beats.push(started.elapsed());
        answering(beats.len())
    });
    let probe = probe();
    let holds = idle_holds();
    let (published, _watching) = watch::channel(Assignment::initial());
    let shutdown = CancellationToken::new();
    let halt = Halt::new(shutdown.clone());
    let beating = Heartbeat::new(&plane, &probe, &holds).keep_beating(&published, &halt);
    let driving = async {
        during(&calls).await;
        tokio::time::sleep_until(started + until).await;
        shutdown.cancel();
    };
    tokio::join!(beating, driving);
    holds.shutdown().await;
    let beats = stamps
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    beats.iter().copied().filter(|at| *at < until).collect()
}

fn ok(interval_ms: u32) -> Answer {
    answer(KEEP_GOING, &policy(), false, interval_ms, &[])
}

/// A policy pushed nine seconds after a beat is applied and the next beat still goes
/// at ten: the round clock alone times the beats.
#[tokio::test(start_paused = true)]
async fn a_pushed_policy_does_not_postpone_the_next_beat() {
    let beats = beats_until(
        Duration::from_millis(10_500),
        |_beat| ok(10_000),
        async |calls| {
            tokio::time::sleep(Duration::from_secs(9)).await;
            calls
                .push_policy(&serde_json::json!({"status": KEEP_GOING,
                    "assigned_policy": policy(), "degraded": false, "degraded_reason": null,
                    "selftest_requested": false, "heartbeat_interval_ms": 10_000,
                    "release_holds": []}))
                .await;
        },
    )
    .await;
    assert_eq!(
        beats,
        [Duration::ZERO, Duration::from_secs(10)],
        "the first beat at once, the next on the round, none on the policy"
    );
}

/// A round that falls while a beat waits for its answer is kept: once the answer
/// lands, exactly one more beat goes at once, and the next follows the round after.
#[tokio::test(start_paused = true)]
async fn a_round_due_during_a_beat_in_flight_still_beats_once() {
    let beats = beats_until(
        Duration::from_millis(9_500),
        |beat| {
            let answered = ok(3_000);
            match (beat, answered) {
                // The second beat, sent on the round at 3 s, is answered at 7 s: the
                // round at 6 s falls while it is in flight.
                (2, Answer::Reply(bytes)) => Answer::Late(Duration::from_secs(4), bytes),
                (_, answered) => answered,
            }
        },
        async |_calls| {},
    )
    .await;
    assert_eq!(
        beats,
        [
            Duration::ZERO,
            Duration::from_secs(3),
            Duration::from_secs(7),
            Duration::from_secs(9),
        ],
        "the kept round beats once when the answer lands, then the clock resumes"
    );
}
