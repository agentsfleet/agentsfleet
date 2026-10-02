#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use afd_core::error_code;
use afd_core::id::Uuid7;
use afd_wire::report::FailureClass;

use super::{RENEWAL_TICK, Renewal};
use crate::client::Verb;
use crate::error;
use crate::test_support::{Answer, LEASE_ID, json, plane};

#[tokio::test(start_paused = true)]
async fn test_renew_keeps_on_5xx_ends_on_4xx() {
    let renewals = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&renewals);
    let (plane, _calls) = plane(move |_call| match counted.fetch_add(1, Ordering::SeqCst) {
        0 => json(&serde_json::json!({"lease_expires_at": 1})),
        1 | 2 => Answer::Fail(error::unavailable(Verb::Renew, 503)),
        _ => Answer::Fail(error::refused(
            Verb::Renew,
            409,
            Some(error_code::RUN_LEASE_LOST),
        )),
    });
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let started = tokio::time::Instant::now();

    let class = Renewal::new(&plane, &lease).keep().await;

    assert_eq!(class, FailureClass::RenewalTerminate);
    assert_eq!(
        renewals.load(Ordering::SeqCst),
        4,
        "two 503s kept the lease; the 409 ended it"
    );
    assert_eq!(started.elapsed(), RENEWAL_TICK * 4);
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

    assert_eq!(
        Renewal::new(&plane, &lease).keep().await,
        FailureClass::BudgetBreach
    );
}
