//! The renewal race: a renewal that never answers gives the claim up
//! before it could lapse.

use std::future::pending;
use std::time::Duration;

use super::hold_claim;
use crate::obligation::{CLAIM_RENEW, CLAIM_TTL};

#[tokio::test(start_paused = true)]
async fn a_renewal_that_never_answers_gives_the_claim_up_before_it_lapses() {
    let started = tokio::time::Instant::now();
    let lost = hold_claim(CLAIM_RENEW, pending::<crate::error::Result<bool>>).await;
    assert_eq!(lost, "renew_unanswered");
    let held = started.elapsed();
    assert!(
        held < CLAIM_TTL,
        "the holder kept the claim {held:?}, past its {CLAIM_TTL:?}"
    );
    assert_eq!(held, CLAIM_TTL.saturating_sub(CLAIM_RENEW / 2));
}

#[tokio::test(start_paused = true)]
async fn a_landed_renewal_extends_from_when_it_was_sent() {
    let started = tokio::time::Instant::now();
    let mut calls = 0;
    let lost = hold_claim(CLAIM_RENEW, || {
        calls += 1;
        let first = calls == 1;
        async move {
            if first {
                // Lands, but only after a slow round trip.
                tokio::time::sleep(Duration::from_secs(4)).await;
                Ok(true)
            } else {
                pending::<crate::error::Result<bool>>().await
            }
        }
    })
    .await;
    assert_eq!(lost, "renew_unanswered");
    // Sent at 10 s and landed at 14 s, so the claim counts from 10 s: the
    // stalled second renewal is given up at 10 + 30 - 5 = 35 s.
    assert_eq!(
        started.elapsed(),
        (CLAIM_RENEW + CLAIM_TTL).saturating_sub(CLAIM_RENEW / 2)
    );
}

#[tokio::test(start_paused = true)]
async fn a_renewal_another_holder_refused_ends_at_once() {
    let started = tokio::time::Instant::now();
    let lost = hold_claim(CLAIM_RENEW, || async { Ok(false) }).await;
    assert_eq!(lost, "renew");
    assert_eq!(started.elapsed(), CLAIM_RENEW);
}
