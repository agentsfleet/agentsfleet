//! An owed answer survives losing what carried it: the worker, its consumer
//! group, or the whole stream.
//!
//! Split from the obligation cases at the file cap.

use super::*;

/// A worker replaced under a different hostname leaves its answer recoverable.
///
/// The dimension's named proof. A consumer name is host-derived and constant for
/// a process, so the replacement reads a pending list that is EMPTY — the dead
/// host's entries are not offered to it, and `read_blocking` never re-offers an
/// entry already handed out. The entry is therefore held by a name that will
/// never come back, and unreachable by anyone else.
///
/// Staged with two explicitly-named readers because one test process has one
/// hostname: the name is the only thing that differs from what production builds.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn test_outbound_obligations_survive_worker_replacement() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let redis = datastore().await;
    let event = "1700000000-3";
    owe_and_queue(&harness, 5, event).await;

    // The host that dies, taking the entry into its own pending list.
    let mut departed = reader_named(&OutboundHarness::config(), "host-that-died").await;
    let taken = departed
        .read_blocking(BLOCK_MS)
        .await
        .expect("the group offers the entry")
        .expect("an entry was queued for it");
    assert_eq!(taken.event_id, event, "the host took this fixture's answer");
    drop(departed);

    assert_eq!(
        harness.pending_count().await,
        1,
        "the dead host still holds it — an entry delivered and never acknowledged"
    );

    // Its replacement, under a new hostname.
    let mut replacement = reader_named(&OutboundHarness::config(), "host-that-replaced-it").await;
    assert!(
        replacement
            .read_pending()
            .await
            .expect("the pending read answers")
            .is_none(),
        "a new hostname inherits NOTHING: the pending list it reads is its own, \
         and the dead host's is not offered to it"
    );
    assert!(
        replacement
            .read_blocking(BLOCK_MS)
            .await
            .expect("the blocking read answers")
            .is_none(),
        "and the entry is never re-offered as new, so no worker can reach it"
    );

    assert_eq!(
        awaiting_delivery(&harness).await,
        vec![event.to_owned()],
        "the queue cannot deliver it and the ledger still owes it — which is \
         the whole claim: the obligation outlives the entry"
    );

    // Recovery is the producer re-appending. Not free — the destination may see
    // the answer twice — which is why `delivered_at` is stamped by the poster
    // and not by the acknowledgement: a row leaves this set only when somebody
    // actually got it.
    let again = harness
        .queue
        .enqueue(OutboundJob {
            provider: PROVIDER,
            destination: DESTINATION,
            workspace_id: WORKSPACE,
            fleet_id: FLEET,
            event_id: event,
            answer: ANSWER,
        })
        .await
        .expect("the re-append is accepted");
    obligation::record_reappended(
        &harness.database,
        &obligation_id(5),
        again.as_str(),
        UnixMillis::from_millis(SEEDED_AT),
    )
    .await
    .expect("recording the re-append");
    obligation::stamp_delivered(
        &harness.database,
        FLEET,
        event,
        UnixMillis::from_millis(SEEDED_AT + 30),
    )
    .await
    .expect("the re-appended answer is delivered");

    assert!(
        awaiting_delivery(&harness).await.is_empty(),
        "re-appended and delivered: the answer survived its worker"
    );
    assert!(
        entries_on(&redis).await >= 1,
        "the re-append put a real entry on the real stream"
    );
}

/// A lost consumer group leaves the answer owed.
///
/// Its own test rather than a branch of the one above, because the fault has to
/// be applied to the whole stream: a group cannot be destroyed for one entry.
/// The entries SURVIVE here and become unreachable, which is a different shape
/// from losing the stream — and the point is that the ledger does not care.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn a_lost_consumer_group_leaves_the_answer_owed() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let redis = datastore().await;
    let event = "1700000000-4";
    owe_and_queue(&harness, 6, event).await;

    forget_group(&redis).await;

    assert_eq!(
        entries_on(&redis).await,
        1,
        "the entry is still there — it is the way to reach it that is gone"
    );
    assert_eq!(
        awaiting_delivery(&harness).await,
        vec![event.to_owned()],
        "queued and unreachable reads, to the ledger, as still owed"
    );
}

/// A wholly lost stream leaves the answer owed.
///
/// The harshest of the three and the one that proves the ledger is the forge:
/// entries, group and pending lists all gone at once, and the answer is still
/// owed because PostgreSQL never stopped knowing about it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live Dragonfly: make test-integration-rustd"]
async fn a_wholly_lost_stream_leaves_the_answer_owed() {
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let redis = datastore().await;
    let event = "1700000000-5";
    owe_and_queue(&harness, 7, event).await;

    forget_stream(&redis).await;

    assert_eq!(entries_on(&redis).await, 0, "nothing of the queue survives");
    assert_eq!(
        awaiting_delivery(&harness).await,
        vec![event.to_owned()],
        "losing the cache entirely erases no obligation"
    );
}
