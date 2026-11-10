//! A lane stopped while it waits for the in-flight ceiling gives its job back.

use std::collections::BTreeSet;

use af_core::test_util::trace::Capture;

use super::*;

/// Every vendor call the ceiling allows is held, so the next lane's job waits
/// for a permit. A shutdown in that wait releases the job's claim without
/// starting its cycle, and the deliveries already in hand still finish.
#[tokio::test]
async fn a_job_waiting_for_a_permit_at_shutdown_gives_its_claim_back_unstarted() {
    let capture = Capture::install();
    let token = CancellationToken::new();
    let poster = Gated::default();
    let lanes = Lanes::new(
        Posters {
            slack: poster.clone(),
        },
        afd_db::test_util::unreachable_db(),
        token.clone(),
    );
    let held: Vec<_> = (1..=IN_FLIGHT_DELIVERIES)
        .map(|w| poster.shut(&workspace(w)))
        .collect();
    for w in 1..=IN_FLIGHT_DELIVERIES {
        lanes.dispatch(job(w, 1)).await;
    }
    await_until("every permit to be held", || {
        poster.in_flight() == IN_FLIGHT_DELIVERIES
    })
    .await;
    let waiting = IN_FLIGHT_DELIVERIES + 1;
    lanes.dispatch(job(waiting, 1)).await;
    // One poll interval lets the waiting lane take its job and park on the
    // ceiling; this runtime has one thread, so nothing else moves meanwhile.
    tokio::time::sleep(POLL).await;

    token.cancel();
    for gate in held {
        gate.notify_one();
    }
    tokio::time::timeout(PATIENCE, lanes.drain())
        .await
        .expect("every lane stops once its delivery in hand ends");

    let expected: BTreeSet<Uuid7> = (1..=IN_FLIGHT_DELIVERIES).map(|w| event(w, 1)).collect();
    assert_eq!(
        poster.delivered().into_iter().collect::<BTreeSet<_>>(),
        expected,
        "the deliveries in hand finished, and the waiting job was never started"
    );
    assert_eq!(poster.high_water(), IN_FLIGHT_DELIVERIES);
    let events = capture.events();
    let named = |name: &str| {
        events
            .iter()
            .filter(|event| event.field("event") == Some(name))
            .count()
    };
    assert_eq!(
        named("outbound_obligation_attempt_failed"),
        IN_FLIGHT_DELIVERIES,
        "only the started jobs counted a cycle"
    );
    let released = capture.only("outbound_claim_release_failed");
    assert_eq!(
        released.field("error_code"),
        Some(af_core::error_code::INTERNAL_DB_UNAVAILABLE.as_str()),
        "the waiting job tried to give its claim back to a ledger that is down"
    );
    released.assert_uncoded_reason();
}
