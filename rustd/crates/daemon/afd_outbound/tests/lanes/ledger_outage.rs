//! What a lane does when the ledger stops answering mid-cycle.

use af_core::error_code::INTERNAL_DB_UNAVAILABLE;
use af_core::test_util::trace::Capture;
use afd_outbound::obligation::{CLAIM_RENEW, CLAIM_TTL};

use super::*;

// The renewal count below is the number of renewals that fit before a claim
// lapses, minus the one that would arrive too late.
const _: () = assert!(CLAIM_TTL.as_secs() == 30 && CLAIM_RENEW.as_secs() == 10);

/// Lanes delivering through `poster` over a ledger nobody answers on.
fn lanes_over_no_ledger(poster: &Gated, token: &CancellationToken) -> Lanes<Gated> {
    Lanes::new(
        Posters {
            slack: poster.clone(),
        },
        afd_db::test_util::unreachable_db(),
        token.clone(),
    )
}

/// The events `capture` holds named `name`.
fn named(capture: &Capture, name: &str) -> usize {
    capture
        .events()
        .iter()
        .filter(|event| event.field("event") == Some(name))
        .count()
}

/// A delivery whose renewals go unanswered keeps trying while the claim still
/// has room, and stops before the claim can lapse: another replica may retake
/// the row, and two holders would both post it.
#[tokio::test(start_paused = true)]
async fn unanswered_renewals_end_the_cycle_before_the_claim_can_lapse() {
    let capture = Capture::install();
    let token = CancellationToken::new();
    let poster = Gated::default();
    let lanes = lanes_over_no_ledger(&poster, &token);
    let _never_answers = poster.shut(&workspace(SLOW_WORKSPACE));
    let started = tokio::time::Instant::now();

    lanes.dispatch(job(SLOW_WORKSPACE, 1)).await;
    tokio::time::timeout(CLAIM_TTL * 2, lanes.finished())
        .await
        .expect("the cycle ends on its own");

    assert!(
        started.elapsed() < CLAIM_TTL,
        "the cycle ended after {:?}, past the claim's {CLAIM_TTL:?}",
        started.elapsed()
    );
    // Renewals at 10 s and 20 s; a third would land after the claim lapsed.
    assert_eq!(named(&capture, "outbound_claim_renew_failed"), 2);
    for event in capture
        .events()
        .iter()
        .filter(|event| event.field("event") == Some("outbound_claim_renew_failed"))
    {
        assert_eq!(
            event.field("error_code"),
            Some(INTERNAL_DB_UNAVAILABLE.as_str())
        );
        event.assert_uncoded_reason();
    }
    let lapsed = capture.only("outbound_claim_lapsed");
    assert_eq!(lapsed.field("write"), Some("renew_unanswered"));
    assert!(
        poster.delivered().is_empty(),
        "the held delivery was abandoned"
    );
    assert_eq!(named(&capture, "outbound_obligation_stamp_failed"), 0);

    token.cancel();
    lanes.drain().await;
}

/// An answer no connector can take is abandoned without a poster; when the
/// ledger cannot record that, the failure is reported and nothing announced.
#[tokio::test]
async fn an_abandonment_the_ledger_cannot_record_is_reported_not_announced() {
    let capture = Capture::install();
    let token = CancellationToken::new();
    let poster = Gated::default();
    let lanes = lanes_over_no_ledger(&poster, &token);
    let mut unaddressable = job(FAST_WORKSPACE, 1);
    unaddressable.outgoing.provider = "carrier-pigeon".to_owned();

    lanes.dispatch(unaddressable).await;
    tokio::time::timeout(PATIENCE, lanes.finished())
        .await
        .expect("the cycle ends");

    let failed = capture.only("outbound_obligation_abandon_failed");
    assert_eq!(
        failed.field("error_code"),
        Some(INTERNAL_DB_UNAVAILABLE.as_str())
    );
    failed.assert_uncoded_reason();
    assert_eq!(named(&capture, "outbound_delivery_abandoned"), 0);
    assert_eq!(
        named(&capture, "outbound_obligation_attempt_failed"),
        0,
        "a row nobody can deliver counts no cycle"
    );
    assert_eq!(poster.high_water(), 0, "no poster was tried");

    token.cancel();
    lanes.drain().await;
}
