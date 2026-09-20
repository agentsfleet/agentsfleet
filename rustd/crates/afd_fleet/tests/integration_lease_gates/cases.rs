//! What the lease verb does with each gate's verdict.

use super::seed::{seed_gate, seed_provider_resolution, seed_spend};
use super::*;

use afd_core::event::label;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_fleet_paused_after_its_event_was_claimed_issues_no_lease() {
    // The window `installed()` documents and no suite had entered: the
    // selection pass filters on status, so a fleet reaching the claim and THEN
    // stopping is an operator pausing it in between. The claim must lapse on
    // its own rather than run under a fleet nobody wants running.
    //
    // The pause lands BETWEEN the claim and the verb, which is the only place
    // it can: a fleet stopped before the claim is never selected at all.
    crate::support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let seeded = ready(&fixtures).await;
    set_config(&fixtures, &seeded.fleet, BUDGETED_CONFIG).await;

    let claimed = claim(&fixtures, &seeded).await;
    set_status(&fixtures, &seeded.fleet, FLEET_STATUS_STOPPED).await;
    let answer = drive(&fixtures, &seeded, claimed).await;

    assert!(
        answer.contains(NO_LEASE),
        "a paused fleet issued a lease: {answer}"
    );
    assert_eq!(
        terminal_of(&fixtures, &seeded.fleet, &seeded.event_id).await,
        None,
        "a fleet paused mid-claim must not open a narrative row for the event"
    );

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_fleet_past_its_ceiling_ends_the_event_rather_than_retrying_it() {
    // `money_gates` is proven against a drained ledger next door. What runs
    // only here is what the verb DOES with that refusal: end the event and
    // record which gate ended it. A retry would leave the delivery leasable,
    // and every poll would re-read the same exhausted ledger forever.
    crate::support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let seeded = ready(&fixtures).await;
    set_config(&fixtures, &seeded.fleet, BUDGETED_CONFIG).await;
    seed_provider_resolution(&fixtures, &seeded.fleet).await;
    seed_spend(&fixtures, &seeded, &seeded.tenant, OVERSPENT_NANOS).await;

    let claimed = claim(&fixtures, &seeded).await;
    let answer = drive(&fixtures, &seeded, claimed).await;

    assert!(
        answer.contains(NO_LEASE),
        "a drained budget issued a lease: {answer}"
    );
    let (status, failure) = terminal_of(&fixtures, &seeded.fleet, &seeded.event_id)
        .await
        .expect("a refused event has a narrative row");
    assert_eq!(status, STATUS_GATE_BLOCKED, "the row must be terminal");
    assert_eq!(
        failure,
        label::BUDGET_BREACH,
        "the row must name the ceiling as what ended it"
    );

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_denied_gate_ends_the_event_and_names_the_denial() {
    // `of_gate` maps a denial to `Admission::Refuse`, and the verb must then
    // end the event: a human said no, so waiting would offer the same event
    // back on every poll forever.
    crate::support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let seeded = ready(&fixtures).await;
    set_config(&fixtures, &seeded.fleet, BUDGETED_CONFIG).await;
    seed_provider_resolution(&fixtures, &seeded.fleet).await;
    seed_gate(&fixtures, &seeded, "denied").await;

    let claimed = claim(&fixtures, &seeded).await;
    let answer = drive(&fixtures, &seeded, claimed).await;

    assert!(
        answer.contains(NO_LEASE),
        "a denied gate issued a lease: {answer}"
    );
    let (status, failure) = terminal_of(&fixtures, &seeded.fleet, &seeded.event_id)
        .await
        .expect("a denied event has a narrative row");
    assert_eq!(status, STATUS_GATE_BLOCKED, "the row must be terminal");
    assert_eq!(
        failure,
        label::APPROVAL_DENIED,
        "a denial must not be recorded as some other refusal"
    );

    fixtures.cleanup().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_an_unanswered_gate_parks_the_event_without_ending_it() {
    // The mirror of the denial above, and the pair is the claim worth having:
    // both are one `Verdict` at the gate, and they must become opposite endings
    // here. Ending an unanswered gate would throw away work a human is still
    // deciding about, so the row must stay runnable.
    crate::support::install_subscriber();
    let fixtures = Fixtures::create_with_queue().await;
    let seeded = ready(&fixtures).await;
    set_config(&fixtures, &seeded.fleet, BUDGETED_CONFIG).await;
    seed_provider_resolution(&fixtures, &seeded.fleet).await;
    seed_gate(&fixtures, &seeded, "pending").await;

    let claimed = claim(&fixtures, &seeded).await;
    let answer = drive(&fixtures, &seeded, claimed).await;

    assert!(
        answer.contains(NO_LEASE),
        "an unanswered gate issued a lease: {answer}"
    );
    let (status, failure) = terminal_of(&fixtures, &seeded.fleet, &seeded.event_id)
        .await
        .expect("a parked event still opens its narrative row");
    assert_eq!(
        status, STATUS_RECEIVED,
        "an unanswered gate must leave the event runnable"
    );
    assert_eq!(failure, "", "a parked event has not failed");

    fixtures.cleanup().await;
}
