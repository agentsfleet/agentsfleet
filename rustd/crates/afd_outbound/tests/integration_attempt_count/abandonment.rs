//! Abandonment: an answer nobody can take is stamped once, and no scan offers
//! it again.
//!
//! Split from the attempt-count cases at the file cap.

use super::capture::*;

use super::*;

/// The age every scan below asks for: none, so a row the scans leave out is
/// one they would leave out at any age.
const ANY_AGE: Duration = Duration::ZERO;

/// Every event either recovery scan would offer, at any age.
async fn offered(harness: &OutboundHarness) -> Vec<String> {
    let unreceipted = obligation::unreceipted(&harness.database, ANY_AGE, AMPLE)
        .await
        .expect("the unreceipted scan answers");
    let undelivered = obligation::undelivered(&harness.database, ANY_AGE, AMPLE)
        .await
        .expect("the undelivered scan answers");
    unreceipted
        .into_iter()
        .chain(undelivered)
        .map(|owed| owed.event_id)
        .collect()
}

/// Runs one delivery cycle of `event` through lanes whose poster answers
/// `verdict`, and waits for its acknowledgement.
async fn one_cycle(harness: &OutboundHarness, entry: EventId, event: &str, verdict: Verdict) {
    let server = HangingQueue::spawn().await;
    let token = CancellationToken::new();
    let poster = Scripted::answering(&[verdict]);
    let lanes = lanes_over(&server, harness.database.clone(), poster, &token).await;
    lanes.dispatch(job(entry.clone(), event)).await;
    await_until("the cycle's job to be acknowledged", || {
        server.acks().contains(&entry.as_str().to_owned())
    })
    .await;
    token.cancel();
    lanes.drain().await;
}

/// Dimension 4.1 — a permanent refusal abandons the row, the abandonment is
/// announced once, and no scan offers the answer again.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn permanent_refusal_abandons_the_obligation() {
    let capture = capture();
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let event = "1700000004-0";
    let entry = owe_and_queue(&harness, 20, event).await;
    let announced_before = capture.named(EVENT_ABANDONED).len();

    one_cycle(&harness, entry, event, Verdict::Permanent).await;

    let (abandoned_at, reason) = abandonment(&harness.database, event).await;
    assert!(abandoned_at.is_some(), "a refusal for good is abandoned");
    assert_eq!(reason.as_deref(), Some(AbandonReason::Refused.as_str()));
    assert!(
        !offered(&harness).await.contains(&event.to_owned()),
        "an abandoned answer is never re-offered, at any cutoff"
    );
    assert_eq!(
        capture.named(EVENT_ABANDONED).len(),
        announced_before + 1,
        "the abandonment is announced once"
    );
    assert_eq!(
        obligation::abandon(&harness.database, FLEET, event, AbandonReason::Refused)
            .await
            .expect("the ledger answers"),
        None,
        "a second abandon stamps nothing, so nothing is announced twice"
    );

    // A duplicate entry for the same answer reaches a lane and is refused
    // again: the row is already abandoned, so the lane says nothing new.
    one_cycle(
        &harness,
        EventId::of("1700000004999-0"),
        event,
        Verdict::Permanent,
    )
    .await;
    assert_eq!(
        capture.named(EVENT_ABANDONED).len(),
        announced_before + 1,
        "a refused duplicate of an abandoned answer is not announced again"
    );
}

/// An abandon stamp the ledger will not write is reported, and the job is
/// acknowledged anyway: the row stays in the lost set, which is the
/// at-least-once direction, and the lane is not left holding a refusal.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_failed_abandon_stamp_is_reported_and_the_job_acknowledged() {
    let capture = capture();
    let event = "1700000004-4";
    let entry = EventId::of("1700000004000-4");
    let server = HangingQueue::spawn().await;
    let token = CancellationToken::new();
    let poster = Scripted::answering(&[Verdict::Permanent]);
    let lanes = lanes_over(&server, no_ledger::no_ledger(), poster, &token).await;

    lanes.dispatch(job(entry.clone(), event)).await;
    await_until(
        "the refused job to be acknowledged despite the dead ledger",
        || server.acks().contains(&entry.as_str().to_owned()),
    )
    .await;
    token.cancel();
    lanes.drain().await;

    assert!(
        !capture.named(EVENT_ABANDON_FAILED).is_empty(),
        "the abandon failure is reported: {:?}",
        capture.events()
    );
}

/// Dimension 4.2 — a destination failing retryably is re-offered while cycles
/// remain, and abandoned on the cycle that reaches the cap.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn exhausted_cycles_abandon_the_obligation() {
    let _capture = capture();
    let _lane = OUTBOUND_LANE.lock().await;
    let harness = ready().await;
    let event = "1700000004-1";
    let entry = owe_and_queue(&harness, 21, event).await;
    // Every cycle but the last two already spent, as the ledger would hold it
    // after that many re-offers.
    set_attempts(&harness, event, MAX_DELIVERY_CYCLES - 2).await;

    one_cycle(&harness, entry, event, Verdict::Retryable).await;
    assert_eq!(
        abandonment(&harness.database, event).await,
        (None, None),
        "one cycle is still left, so the answer is still owed"
    );
    assert!(
        offered(&harness).await.contains(&event.to_owned()),
        "and the scan still offers it"
    );

    one_cycle(
        &harness,
        EventId::of("1700000004999-1"),
        event,
        Verdict::Retryable,
    )
    .await;
    let (attempts, delivered_at, _updated_at) = row(&harness, event).await;
    assert_eq!(
        attempts, MAX_DELIVERY_CYCLES,
        "the capping cycle was counted"
    );
    assert_eq!(delivered_at, None);
    let (abandoned_at, reason) = abandonment(&harness.database, event).await;
    assert!(
        abandoned_at.is_some(),
        "the cycle that reached the cap abandoned it"
    );
    assert_eq!(
        reason.as_deref(),
        Some(AbandonReason::CyclesExhausted.as_str())
    );
    assert!(
        !offered(&harness).await.contains(&event.to_owned()),
        "an answer out of cycles is never re-offered"
    );
}

/// Sets the row's spent delivery cycles, as that many re-offers would leave it.
pub(super) async fn set_attempts(harness: &OutboundHarness, event: &str, cycles: i64) {
    let mut connection = harness
        .database
        .acquire()
        .await
        .expect("the ledger answers");
    sqlx::query(
        "UPDATE fleet.obligations SET attempt_count = $3
          WHERE fleet_id = $1::uuid AND event_id = $2::text",
    )
    .bind(FLEET)
    .bind(event)
    .bind(cycles)
    .execute(&mut *connection)
    .await
    .expect("seeding the spent cycles");
}
