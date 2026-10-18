//! A pending physical effect resolves accounting through the same retained report.

use super::{Attempt, Fixture, accounting, balance, counts, fixture, messages, report};
use afd_authoring::types::Source;
use afd_core::error_code;
use afd_wire::authoring::{Outcome, Report};
use std::{sync::Arc, time::Duration};

/// One millisecond of runtime; the provider is billed to the owner's key.
const RUNTIME_DEBIT: i64 = 100;

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn retained_unknown_report_settles_late_known_usage_once() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    let mut stub = fixture::Provider::known();
    stub.blocked = true;
    let provider = Arc::new(stub);
    let calls = fixture::calls(&f, &a, Arc::clone(&provider)).await;
    let task = tokio::spawn({
        let (runner, lease, request) = (f.runner.clone(), a.lease.clone(), fixture::request(&a));
        async move { calls.call(&runner, &lease, request).await }
    });
    tokio::time::timeout(Duration::from_secs(5), provider.arrived())
        .await
        .expect("dispatch reached boundary");
    let request = report();
    let before = balance(&f).await;
    acknowledged_unknown(&f, &a, before, &request).await;
    provider.resume();
    assert_eq!(
        task.await
            .expect("late task")
            .expect_err("late text fenced")
            .code(),
        error_code::AUTHORING_FENCE
    );
    settled_by_the_same_report(&f, &a, before, &request).await;
    assert_eq!(provider.count(), 1);
    f.cleanup().await;
}

/// While the dispatch is pending, the report and its replay stay unsettled.
async fn acknowledged_unknown(f: &Fixture, a: &Attempt, before: i64, request: &Report) {
    let ack = f
        .drafts
        .report(&f.runner, &a.lease, request.clone())
        .await
        .expect("unknown acknowledgement");
    assert!(!ack.settled);
    assert_eq!(counts(f, a).await, (0, 1, 0));
    assert_eq!(balance(f).await, before);
    assert!(
        outstanding(f, a).await,
        "unknown usage keeps the obligation"
    );
    let failure = messages(f, a)
        .await
        .into_iter()
        .find_map(|m| m.failure)
        .expect("durable recovery action");
    assert_eq!(failure.error_code, error_code::AUTHORING_UNKNOWN);
    assert_eq!(failure.recovery_action, "await_accounting");
    let early = f
        .drafts
        .report(&f.runner, &a.lease, request.clone())
        .await
        .expect("replay while still pending");
    assert!(!early.settled && early.replayed);
    assert_eq!(balance(f).await, before);
}

async fn settled_by_the_same_report(f: &Fixture, a: &Attempt, before: i64, request: &Report) {
    let (first, racing) = tokio::join!(
        f.drafts.report(&f.runner, &a.lease, request.clone()),
        f.drafts.report(&f.runner, &a.lease, request.clone())
    );
    for ack in [first, racing] {
        let ack = ack.expect("retained report settles late usage");
        assert!(ack.settled && ack.replayed);
        assert_eq!(ack.state, Outcome::Failed);
    }
    assert_eq!(counts(f, a).await, (1, 1, 0));
    assert_eq!(balance(f).await, before - RUNTIME_DEBIT);
    assert!(
        !outstanding(f, a).await,
        "the draft can admit another attempt"
    );
    let again = f
        .drafts
        .report(&f.runner, &a.lease, accounting(request))
        .await
        .expect("accounting-only replay reads the settlement");
    assert!(again.settled && again.replayed);
    assert_eq!(again.state, Outcome::Failed);
    assert_eq!(balance(f).await, before - RUNTIME_DEBIT);
    let mut changed = accounting(request);
    if let Report::Accounting { accounting } = &mut changed {
        accounting.usage.runtime_ms = 2;
    }
    assert_eq!(
        f.drafts
            .report(&f.runner, &a.lease, changed)
            .await
            .expect_err("changed body under the same identity")
            .code(),
        error_code::AUTHORING_REPLAY
    );
    assert_eq!(counts(f, a).await, (1, 1, 0));
    assert_eq!(balance(f).await, before - RUNTIME_DEBIT);
    without_text(f, a).await;
}

async fn without_text(f: &Fixture, a: &Attempt) {
    let saved = messages(f, a).await;
    assert!(
        saved
            .iter()
            .filter(|m| m.role == "assistant")
            .all(|m| m.text.is_none())
    );
    assert!(
        saved
            .iter()
            .filter_map(|m| m.failure.as_ref())
            .all(|failure| failure.recovery_action == "retry_message")
    );
}

/// The admission predicate: an unsettled or unknown attempt blocks the draft.
async fn outstanding(f: &Fixture, a: &Attempt) -> bool {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM authoring.attempts WHERE draft_id=$1::uuid \
        AND (status<>'settled' OR usage_unknown))",
    )
    .bind(a.draft.id.as_str())
    .fetch_one(&mut *f.connection().await)
    .await
    .expect("attempt obligation")
}
