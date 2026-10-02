#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use afd_core::clock::{FixedClock, UnixMillis};
use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_wire::report::FailureClass;
use tokio::time::Instant;

use super::{EXPIRY_MARGIN, RENEWAL_TICK, Renewal};
use crate::client::Verb;
use crate::error;
use crate::test_support::{Answer, GRANTED_UNTIL, LEASE_ID, json, plane};

/// The grant every successful renewal answers with: thirty seconds after the
/// fixed clock's zero, so each one pushes the deadline a full grant ahead.
fn renewed() -> Answer {
    json(&serde_json::json!({"lease_expires_at": GRANTED_UNTIL}))
}

fn clock() -> FixedClock {
    FixedClock::at(UnixMillis::from_millis(0))
}

/// What the lease is granted for, less the margin it is given up early by.
fn budget() -> Duration {
    Duration::from_millis(GRANTED_UNTIL.unsigned_abs()).saturating_sub(EXPIRY_MARGIN)
}

#[tokio::test(start_paused = true)]
async fn test_renew_keeps_on_5xx_ends_on_4xx() {
    let renewals = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&renewals);
    let (plane, _calls) = plane(move |_call| match counted.fetch_add(1, Ordering::SeqCst) {
        0 => renewed(),
        1 | 2 => Answer::Fail(error::unavailable(Verb::Renew, 503)),
        _ => Answer::Fail(error::refused(
            Verb::Renew,
            409,
            Some(error_code::RUN_LEASE_LOST),
        )),
    });
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let clock = clock();
    let started = Instant::now();

    let class = Renewal::new(&plane, &lease, GRANTED_UNTIL, &clock)
        .keep()
        .await;

    assert_eq!(class, FailureClass::RenewalTerminate);
    assert_eq!(
        renewals.load(Ordering::SeqCst),
        4,
        "two 503s kept the lease; the 409 ended it"
    );
    assert_eq!(started.elapsed(), RENEWAL_TICK * 4);
}

#[tokio::test(start_paused = true)]
async fn blips_past_the_granted_deadline_end_the_lease() {
    let (plane, _calls) = plane(|_call| Answer::Fail(error::unavailable(Verb::Renew, 503)));
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let clock = clock();
    let started = Instant::now();

    let class = Renewal::new(&plane, &lease, GRANTED_UNTIL, &clock)
        .keep()
        .await;

    assert_eq!(class, FailureClass::RenewalTerminate);
    assert_eq!(
        started.elapsed(),
        budget(),
        "given up at the deadline, less the margin"
    );
}

#[tokio::test(start_paused = true)]
async fn a_hanging_renewal_does_not_outlive_the_deadline() {
    let (plane, _calls) = plane(|_call| Answer::Stall);
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let clock = clock();
    let started = Instant::now();

    Renewal::new(&plane, &lease, GRANTED_UNTIL, &clock)
        .keep()
        .await;

    assert_eq!(started.elapsed(), budget());
}

#[tokio::test(start_paused = true)]
async fn each_grant_moves_the_deadline_and_a_lapsed_lease_ends_at_once() {
    let renewals = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&renewals);
    let (plane, _calls) = plane(move |_call| {
        if counted.fetch_add(1, Ordering::SeqCst) < 10 {
            renewed()
        } else {
            Answer::Fail(error::unavailable(Verb::Renew, 503))
        }
    });
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let clock = clock();
    let started = Instant::now();

    Renewal::new(&plane, &lease, GRANTED_UNTIL, &clock)
        .keep()
        .await;
    let lapsed = Instant::now();
    Renewal::new(&plane, &lease, -1, &clock).keep().await;

    assert_eq!(
        started.elapsed(),
        RENEWAL_TICK * 10 + budget(),
        "ten grants, then one budget of blips"
    );
    assert_eq!(
        lapsed.elapsed(),
        Duration::ZERO,
        "a grant already past ends the lease at once"
    );
}

#[tokio::test(start_paused = true)]
async fn an_exhausted_budget_ends_the_run_as_a_budget_breach() {
    let (plane, _calls) = plane(|_call| {
        Answer::Fail(error::refused(
            Verb::Renew,
            402,
            Some(error_code::RUN_BUDGET_EXCEEDED),
        ))
    });
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let clock = clock();

    let class = Renewal::new(&plane, &lease, GRANTED_UNTIL, &clock)
        .keep()
        .await;

    assert_eq!(class, FailureClass::BudgetBreach);
}
