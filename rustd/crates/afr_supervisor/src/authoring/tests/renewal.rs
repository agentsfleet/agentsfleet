//! Renewal holds the live slot only while the daemon answers, never past the
//! attempt's end, and stops when the daemon asks it to.
//!
//! On paused time: every wait below is the loop's own, measured exactly.

use super::{ack, json, lease, rig, run};
use crate::{
    authoring::{meter::Observed, renew},
    client::{Call, Verb},
    error,
    test_support::{Answer, clock, plane},
};
use afd_core::id::Uuid7;
use afd_wire::authoring::Lease;
use serde_json::Value;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::time::Instant;

/// A minute past the rig clock's zero: beyond the fixture lease's expiry.
const EXTENDED: &str = "1970-01-01T00:01:00Z";
/// How long the loop waits between successful renewals (`renew::INTERVAL`).
const INTERVAL: Duration = Duration::from_secs(10);
/// How long the loop waits before retrying a retryable refusal (`renew::RETRY`).
const RETRY: Duration = Duration::from_millis(250);

fn renewed(stop_requested: bool) -> Answer {
    json(
        &serde_json::json!({"expires_at": EXTENDED, "stop_requested": stop_requested,
        "remaining_output_tokens": 1, "model_access_expires_at": EXTENDED}),
    )
}

/// Runs the renewal loop for `lease` against a daemon answering its `n`th
/// renewal with `answer(n)`; returns the loop's result and when each renewal
/// was sent, measured from the start.
async fn keep(
    lease: &Lease<Value>,
    answer: impl Fn(usize) -> Answer + Send + Sync + 'static,
) -> (crate::Result<()>, Vec<Duration>) {
    let start = Instant::now();
    let sent = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&sent);
    let (plane, _calls) = plane(move |call: &Call| {
        assert_eq!(
            call.verb,
            Verb::AuthoringRenew,
            "renewal asks for nothing else"
        );
        let mut log = log.lock().unwrap();
        log.push(start.elapsed());
        answer(log.len() - 1)
    });
    let id = Uuid7::parse(super::LEASE).unwrap();
    let kept = renew::keep(&plane, &id, lease, clock().as_ref(), &Observed::new()).await;
    let sent = sent.lock().unwrap().clone();
    (kept, sent)
}

/// The fixture lease with its expiry and attempt end replaced.
fn lease_until(expires_at: &str, deadline: &str) -> Lease<Value> {
    let mut wire = serde_json::to_value(lease()).unwrap();
    wire["expires_at"] = serde_json::json!(expires_at);
    wire["deadline"] = serde_json::json!(deadline);
    serde_json::from_value(wire).unwrap()
}

#[tokio::test(start_paused = true)]
async fn renewal_extends_the_slot_every_interval_until_the_daemon_asks_it_to_stop() {
    let (kept, sent) = keep(&lease(), |n| renewed(n > 0)).await;
    kept.unwrap();
    assert_eq!(sent, [INTERVAL, INTERVAL * 2]);
}

#[tokio::test(start_paused = true)]
async fn a_retryable_refusal_is_retried_quickly_and_a_final_refusal_ends_renewal() {
    let (kept, sent) = keep(&lease(), |n| {
        Answer::Fail(if n == 0 {
            error::unavailable(Verb::AuthoringRenew, 503)
        } else {
            error::refused(Verb::AuthoringRenew, 409, None)
        })
    })
    .await;
    assert!(
        !kept.unwrap_err().is_retryable(),
        "the final refusal ends it"
    );
    assert_eq!(sent, [INTERVAL, INTERVAL + RETRY]);
}

#[tokio::test(start_paused = true)]
async fn a_renewal_that_never_answers_ends_at_the_live_expiry() {
    let start = Instant::now();
    let (kept, sent) = keep(&lease(), |_| Answer::Stall).await;
    kept.unwrap_err();
    assert_eq!(sent, [INTERVAL]);
    assert_eq!(
        start.elapsed(),
        Duration::from_secs(30),
        "the fixture lease's expiry"
    );
}

#[tokio::test(start_paused = true)]
async fn a_slot_that_expires_before_its_first_renewal_ends_without_asking() {
    let start = Instant::now();
    let lease = lease_until("1970-01-01T00:00:05Z", super::ATTEMPT_ENDS);
    let (kept, sent) = keep(&lease, |_| renewed(false)).await;
    kept.unwrap_err();
    assert!(
        sent.is_empty(),
        "no renewal was due before the slot expired"
    );
    assert_eq!(start.elapsed(), Duration::from_secs(5));
}

#[tokio::test(start_paused = true)]
async fn renewal_never_holds_the_slot_past_the_attempt_end() {
    let start = Instant::now();
    let lease = lease_until(super::GRANTED_UNTIL, "1970-01-01T00:00:15Z");
    let (kept, sent) = keep(&lease, |_| renewed(false)).await;
    kept.unwrap_err();
    assert_eq!(sent, [INTERVAL], "the daemon's longer extension is capped");
    assert_eq!(
        start.elapsed(),
        Duration::from_secs(15),
        "the attempt's end"
    );
}

#[tokio::test]
async fn a_halt_ends_a_running_turn_and_reports_its_usage_unknown() {
    let mut rig = rig(|call| match call.verb {
        Verb::AuthoringModel => Answer::Stall,
        Verb::AuthoringRenew => renewed(false),
        Verb::AuthoringReport => ack(),
        other => panic!("unexpected authority {other:?}"),
    });
    rig.lessee.halt.running().cancel();
    let (_, report) = run(&mut rig, lease()).await;
    assert_eq!(report["outcome"], "failed");
    assert_eq!(report["failure_reason"], "provider_usage_unknown");
    assert_eq!(report["usage"]["knowledge"], "unknown");
}

#[tokio::test]
async fn a_lost_slot_ends_a_running_turn_and_reports_its_usage_unknown() {
    let mut rig = rig(|call| match call.verb {
        Verb::AuthoringModel => Answer::Stall,
        Verb::AuthoringReport => ack(),
        other => panic!("an expired slot asked for {other:?}"),
    });
    let expired = lease_until("1970-01-01T00:00:00Z", super::ATTEMPT_ENDS);
    let (calls, report) = run(&mut rig, expired).await;
    assert!(
        calls.iter().all(|call| call.verb != Verb::AuthoringRenew),
        "an expired slot is never renewed"
    );
    assert_eq!(report["outcome"], "failed");
    assert_eq!(report["failure_reason"], "provider_usage_unknown");
}

#[tokio::test(start_paused = true)]
async fn a_turn_that_reaches_its_attempt_end_reports_timed_out() {
    let mut rig = rig(|call| match call.verb {
        Verb::AuthoringModel => Answer::Stall,
        Verb::AuthoringRenew => renewed(false),
        Verb::AuthoringReport => ack(),
        other => panic!("unexpected authority {other:?}"),
    });
    let ends = lease_until(super::GRANTED_UNTIL, "1970-01-01T00:00:15Z");
    let (_, report) = run(&mut rig, ends).await;
    assert_eq!(report["outcome"], "timed_out");
    assert_eq!(report["failure_reason"], "attempt_timed_out");
}
