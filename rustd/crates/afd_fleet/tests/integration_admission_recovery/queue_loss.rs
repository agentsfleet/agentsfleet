//! A queue that lost its entries replays them, and settlement still happens
//! once per event.
//!
//! Split from the crash-boundary case beside it at the file cap.

use super::*;

/// Dimension 2.2 — destroying the queue's data and rebuilding it replays every
/// unfinished admission, with one settlement and one debit.
///
/// The run that already settled is the subject. Its entry is destroyed along
/// with everyone else's, and the only thing distinguishing it from accepted
/// work that must be re-appended is `delivered_at` — stamped by the real
/// `record_received`, not by this test. Without that column recovery re-queues
/// a completed run, and the wallet pays for it twice.
///
/// `forget` is the loss: `DEL` on the stream key takes the entries and the
/// consumer group with them, scoped to this test's own minted fleet, which is
/// the isolated equivalent of a flush and the only one permitted against a
/// shared datastore.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn queue_loss_replays_without_duplicate_settlement() {
    let _lane = RECOVERY_LANE.lock().await;
    let fixtures = Fixtures::create_with_queue().await;
    let (fleet, workspace, tenant, [runner]) = seeded_parts::<1>(&fixtures).await;
    let streams = FleetStreams::new(fixtures.queue().clone());
    let leases = fixtures.leases();
    let live = ledger(&fixtures);
    let now = UnixMillis::from_millis(ENROLLED_AT);
    fixtures.seed_wallet(&tenant, DEEP_POOL, ENROLLED_AT).await;

    // Three admissions: the lease will take one and run it to settlement, and
    // the other two are the accepted work the loss must not lose.
    let mut owed = Vec::new();
    for delivery in ["lost-one", "lost-two", "lost-three"] {
        let key = producer_key(&fleet, delivery);
        let event = live
            .admit(admission(&fleet, &workspace, &key))
            .await
            .expect("a live queue admits and receipts");
        assert!(
            fixtures
                .admission_receipt(&fleet, &event.id)
                .await
                .is_some(),
            "{key} was receipted before the loss"
        );
        owed.push(event.id);
    }

    // ── The run that completes, driven through the real verbs. Extracted
    // because it is not about recovery: it is admission and settlement running
    // to completion,
    // which is the precondition the loss below is injected into.
    let completed = run_one_to_settlement(&fixtures, &leases, &fleet, &tenant, &runner, now).await;
    let before = owed.len();
    owed.retain(|pending| pending != &completed.event_id);
    assert_eq!(
        owed.len(),
        before - 1,
        "the lease took one of the admitted events, and it is no longer owed"
    );
    assert_eq!(owed.len(), 2, "two admissions are still owed to the queue");

    // ── The loss. Entries and consumer group, gone.
    streams
        .forget(&fleet)
        .await
        .expect("destroying this fleet's stream data");
    assert!(
        queue::entries_on(fixtures.queue(), &fleet).await.is_empty(),
        "the stream holds nothing after the loss"
    );

    // ── The rebuild. Reconcile forgets the receipts whose entries are gone;
    // replay re-appends the rows that are owed again.
    // Both passes on the REAL clock, for the reason the 2.1 test states: the
    // ledger stamps `created_at` with `clock::now()`, so a replay cutoff built
    // from the fixture's instant scans nothing.
    let repairing_at = clock::now();
    live.reconcile(
        repairing_at,
        EVERY_FLEET,
        EVERY_ROW,
        &mut Progress::default(),
    )
    .await
    .expect("the reconcile pass runs against both live datastores");
    live.replay(repairing_at, NO_GRACE, EVERY_ROW)
        .await
        .expect("the replay pass runs against both live datastores");

    // Zero accepted work missing: each owed row names a live entry again, once.
    let entries = queue::entries_on(fixtures.queue(), &fleet).await;
    assert_recovered(&fixtures, &streams, &fleet, &owed, &entries, 1).await;

    // The run that already settled was not re-queued. This is the assertion
    // `delivered_at` exists for.
    assert_eq!(
        fixtures
            .admission_receipt(&fleet, &completed.event_id)
            .await,
        Some(completed.receipt.clone()),
        "a delivered admission keeps the receipt it ran under, gone entry or not"
    );
    assert_eq!(
        fixtures
            .admission_replays(&fleet, &completed.event_id)
            .await,
        0,
        "a delivered admission is never re-appended"
    );
    assert!(
        !entries
            .iter()
            .any(|(_receipt, event_id)| event_id == &completed.event_id),
        "nothing re-queued the run that already completed: {entries:?}"
    );

    // One settlement and one debit: the wallet did not move across the loss and
    // the rebuild, and the run's ledger rows did not multiply.
    assert_eq!(
        fixtures.balance(&tenant).await,
        completed.balance,
        "recovery charged the tenant nothing: the only run that ran was settled before the loss"
    );
    assert_eq!(
        fixtures.ledger_rows(&completed.event_id).await,
        completed.debits,
        "the completed run's debits did not multiply across the rebuild"
    );

    // The ledger never grew: recovery re-appends entries, never identities.
    assert_eq!(
        fixtures.admissions_for(&fleet).await,
        3,
        "three producer keys are still three ledger rows after a full rebuild"
    );

    streams.forget(&fleet).await.expect("purging the stream");
    queue::clear_ready(fixtures.queue(), &fleet).await;
    fixtures.cleanup().await;
}
