//! What the lease verb does with each gate's verdict.

use super::*;

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
