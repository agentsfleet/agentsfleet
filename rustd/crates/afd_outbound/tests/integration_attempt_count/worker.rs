//! The worker half: `Lanes` driven with a scripted poster, counting one cycle
//! per delivery however many internal retries it spent.
//!
//! Split from the attempt-count cases at the file cap.

use super::capture::*;

use super::*;

/// One cycle is one count, however many vendor retries it made inside.
///
/// The definition's load-bearing half. A destination that answers 5xx twice
/// and then takes the answer made the poster work three times; the ledger
/// records ONE cycle, the stamp lands, and the delivered event carries that
/// count. A per-request counter would say three and would have cost a ledger
/// write on every rate-limit sleep to say it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn internal_retries_are_one_cycle_and_the_count_rides_the_delivered_event() {
    let capture = capture();
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let event = "1700000002-0";
    let entry = owe_and_queue(&harness, 4, event).await;
    let server = HangingQueue::spawn().await;
    let token = CancellationToken::new();
    let poster = Scripted::answering(&[Verdict::Retryable, Verdict::Retryable, Verdict::Delivered]);
    let lanes = lanes_over(&server, harness.database.clone(), poster.clone(), &token).await;

    lanes.dispatch(job(entry.clone(), event)).await;
    await_until("the delivered answer to be acknowledged", || {
        server.acks().contains(&entry.as_str().to_owned())
    })
    .await;
    token.cancel();
    lanes.drain().await;

    assert_eq!(poster.calls(), 3, "two refusals and an acceptance");
    let (attempts, delivered_at, _updated_at) = row(&harness, event).await;
    assert_eq!(
        attempts, 1,
        "three vendor calls inside one cycle count once"
    );
    assert!(
        delivered_at.is_some(),
        "the destination took it, so it is stamped"
    );
    let delivered = capture.named(EVENT_DELIVERED);
    assert!(
        delivered.iter().any(|seen| seen.attempts == Some(1)),
        "the delivered event carries the recorded count: {delivered:?}"
    );
}

/// An exhausted cycle is counted, acknowledged, and reported with its count.
///
/// The row an operator is looking for. Three refusals spend the budget; the
/// job is acknowledged so it does not park at the head of the lane; the
/// obligation stays undelivered with a count of ONE — which under the old
/// statement would have read zero — and the exhausted warning names that count.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_exhausted_cycle_is_counted_and_reported() {
    let capture = capture();
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let event = "1700000002-1";
    let entry = owe_and_queue(&harness, 5, event).await;
    let server = HangingQueue::spawn().await;
    let token = CancellationToken::new();
    let poster = Scripted::answering(&[Verdict::Retryable]);
    let lanes = lanes_over(&server, harness.database.clone(), poster.clone(), &token).await;

    lanes.dispatch(job(entry.clone(), event)).await;
    await_until("the exhausted job to be acknowledged", || {
        server.acks().contains(&entry.as_str().to_owned())
    })
    .await;
    token.cancel();
    lanes.drain().await;

    assert_eq!(
        poster.calls(),
        DELIVERY_ATTEMPTS,
        "the whole budget was spent"
    );
    let (attempts, delivered_at, _updated_at) = row(&harness, event).await;
    assert_eq!(attempts, 1, "a failed cycle is still a cycle");
    assert_eq!(
        delivered_at, None,
        "nothing was delivered, so nothing is stamped"
    );
    let exhausted = capture.named(EVENT_EXHAUSTED);
    assert!(
        exhausted.iter().any(|seen| seen.attempts == Some(1)),
        "the exhausted event carries the recorded count: {exhausted:?}"
    );
}

/// A permanent refusal is terminal on the first try and still counts once.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_permanent_refusal_counts_one_cycle_and_is_acknowledged() {
    let _capture = capture();
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let event = "1700000002-2";
    let entry = owe_and_queue(&harness, 6, event).await;
    let server = HangingQueue::spawn().await;
    let token = CancellationToken::new();
    let poster = Scripted::answering(&[Verdict::Permanent]);
    let lanes = lanes_over(&server, harness.database.clone(), poster.clone(), &token).await;

    lanes.dispatch(job(entry.clone(), event)).await;
    await_until("the refused job to be acknowledged", || {
        server.acks().contains(&entry.as_str().to_owned())
    })
    .await;
    token.cancel();
    lanes.drain().await;

    assert_eq!(poster.calls(), 1, "a permanent verdict is not retried");
    let (attempts, delivered_at, _updated_at) = row(&harness, event).await;
    assert_eq!(attempts, 1);
    assert_eq!(delivered_at, None);
}

/// A shutdown mid-retry hands the entry back and keeps the cycle it counted.
///
/// The token is cancelled while the poster is still refusing, so `when` stops
/// the retries and the verdict comes back `Retryable` with the token cancelled.
/// That branch acknowledges NOTHING: the entry stays in this consumer's pending
/// list for the next process. The count already recorded stands — the next
/// process's cycle will count a second one, which is true.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn shutdown_requeue_preserves_the_pending_entry() {
    let capture = capture();
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let event = "1700000002-3";
    let entry = owe_and_queue(&harness, 7, event).await;
    let server = HangingQueue::spawn().await;
    let token = CancellationToken::new();
    let poster = Scripted::answering(&[Verdict::Retryable]);
    let lanes = lanes_over(&server, harness.database.clone(), poster.clone(), &token).await;

    lanes.dispatch(job(entry.clone(), event)).await;
    // Cancel as soon as the first refusal has been given: the retry loop reads
    // the token before its next attempt and stops there.
    await_until("the poster to be asked once", || poster.calls() >= 1).await;
    token.cancel();
    lanes.drain().await;

    assert!(
        server.acks().is_empty(),
        "a job handed back at shutdown is not acknowledged: {:?}",
        server.acks()
    );
    let (attempts, delivered_at, _updated_at) = row(&harness, event).await;
    assert_eq!(attempts, 1, "the cycle that was cut short still started");
    assert_eq!(delivered_at, None);
    let requeued = capture.named(EVENT_REQUEUED);
    assert!(
        requeued.iter().any(|seen| seen.attempts == Some(1)),
        "the requeue event carries the recorded count: {requeued:?}"
    );
}

/// A ledger that will not answer costs the count, never the answer.
///
/// Both bookkeeping writes fail — the cycle start and the success stamp — and
/// the answer is still delivered and still acknowledged. The count is not
/// recorded, which the delivered event says by carrying no count rather than a
/// wrong one, and both failures are reported by name.
#[tokio::test(flavor = "multi_thread")]
async fn bookkeeping_failure_does_not_discard_an_answer() {
    let capture = capture();
    let event = "1700000003-0";
    let entry = EventId::of("1700000003000-0");
    let server = HangingQueue::spawn().await;
    let token = CancellationToken::new();
    let poster = Scripted::answering(&[Verdict::Delivered]);
    let lanes = lanes_over(&server, no_ledger::no_ledger(), poster.clone(), &token).await;

    lanes.dispatch(job(entry.clone(), event)).await;
    await_until(
        "the answer to be acknowledged despite the dead ledger",
        || server.acks().contains(&entry.as_str().to_owned()),
    )
    .await;
    token.cancel();
    lanes.drain().await;

    assert_eq!(poster.calls(), 1, "the destination was given the answer");
    assert!(
        !capture.named(EVENT_COUNT_FAILED).is_empty(),
        "the cycle-start failure is reported: {:?}",
        capture.events()
    );
    assert!(
        !capture.named(EVENT_STAMP_FAILED).is_empty(),
        "the stamp failure is reported: {:?}",
        capture.events()
    );
    assert!(
        capture.named(EVENT_DELIVERED).is_empty(),
        "with the stamp refused, no delivered event claims a count"
    );
}
