//! A lost terminal acknowledgement and post-deletion accounting share one debit.

use super::{Attempt, Fixture, accounting, balance, counts, fixture, messages, report};
use afd_authoring::types::Source;
use afd_core::error_code;
use std::sync::Arc;

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn concurrent_terminal_retry_and_deleted_accounting_replay_charge_once() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    let provider = Arc::new(fixture::Provider::known());
    let calls = fixture::calls(&f, &a, Arc::clone(&provider)).await;
    calls
        .call(&f.runner, &a.lease, fixture::request(&a))
        .await
        .expect("known physical receipt");
    let request = report();
    let before = balance(&f).await;
    let (first, retry) = tokio::join!(
        f.drafts.report(&f.runner, &a.lease, request.clone()),
        f.drafts.report(&f.runner, &a.lease, request.clone())
    );
    let first = first.expect("first terminal acknowledgement");
    let retry = retry.expect("serialized exact acknowledgement");
    assert!(first.settled && retry.settled);
    assert_ne!(first.replayed, retry.replayed);
    assert_eq!(
        balance(&f).await,
        before - 100,
        "one millisecond of runtime, provider billed by owner key"
    );
    assert_eq!(counts(&f, &a).await, (1, 1, 1));
    let text = messages(&f, &a)
        .await
        .into_iter()
        .find(|m| m.role == "assistant")
        .expect("saved assistant");
    assert_eq!(text.text.as_deref(), Some("A retained private answer.\n"));
    changed_answer_refuses(&f, &a, &request).await;
    f.drafts
        .delete(f.access(), &a.draft.id)
        .await
        .expect("private purge");
    let replay = f
        .drafts
        .report(&f.runner, &a.lease, accounting(&request))
        .await
        .expect("content-free replay after purge");
    assert!(replay.replayed && replay.settled);
    assert_eq!(counts(&f, &a).await, (1, 1, 0));
    assert_eq!(balance(&f).await, before - 100);
    assert_eq!(provider.count(), 1);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn first_accounting_report_after_deletion_settles_without_creating_private_rows() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    let _calls = fixture::calls(&f, &a, Arc::new(fixture::Provider::known())).await;
    let request = accounting(&report());
    f.drafts
        .delete(f.access(), &a.draft.id)
        .await
        .expect("delete before first report");
    let before = balance(&f).await;
    let ack = f
        .drafts
        .report(&f.runner, &a.lease, request.clone())
        .await
        .expect("first accounting-only settle");
    assert!(ack.settled && !ack.replayed);
    assert_eq!(f.counts().await, (0, 0, 0));
    assert_eq!(counts(&f, &a).await, (1, 1, 0));
    assert_eq!(balance(&f).await, before - 100);
    let mut changed = request;
    if let afd_wire::authoring::Report::Accounting { accounting } = &mut changed {
        accounting.usage.runtime_ms = 2;
    }
    assert_eq!(
        f.drafts
            .report(&f.runner, &a.lease, changed)
            .await
            .expect_err("altered accounting conflicts")
            .code(),
        error_code::AUTHORING_REPLAY
    );
    assert_eq!(balance(&f).await, before - 100);
    f.cleanup().await;
}

async fn changed_answer_refuses(f: &Fixture, a: &Attempt, request: &afd_wire::authoring::Report) {
    let mut changed = request.clone();
    if let afd_wire::authoring::Report::Terminal { answer, .. } = &mut changed {
        *answer = Some("changed".into());
    }
    assert_eq!(
        f.drafts
            .report(&f.runner, &a.lease, changed)
            .await
            .expect_err("changed answer refuses")
            .code(),
        error_code::AUTHORING_REPLAY
    );
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn first_accounting_report_after_private_expiry_charges_its_real_usage_once() {
    /// One millisecond of runtime; the provider is billed to the owner's key.
    const RUNTIME_DEBIT: i64 = 100;
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    let provider = Arc::new(fixture::Provider::known());
    let calls = fixture::calls(&f, &a, Arc::clone(&provider)).await;
    calls
        .call(&f.runner, &a.lease, fixture::request(&a))
        .await
        .expect("a billable dispatch with a known receipt");
    expire_without_a_report(&f, &a).await;
    f.drafts
        .maintain()
        .await
        .expect("production maintenance pass");
    assert_eq!(f.counts().await, (0, 0, 0), "private rows expired first");
    assert_eq!(attempt_status(&f, &a).await, "awaiting_accounting");
    let before = balance(&f).await;
    let request = accounting(&report());
    let first = f
        .drafts
        .report(&f.runner, &a.lease, request.clone())
        .await
        .expect("the first report after expiry settles");
    assert!(first.settled && !first.replayed);
    assert_eq!(balance(&f).await, before - RUNTIME_DEBIT);
    assert_eq!(
        counts(&f, &a).await,
        (1, 1, 0),
        "settled with no private answer"
    );
    assert_eq!(attempt_status(&f, &a).await, "settled");
    let replay = f
        .drafts
        .report(&f.runner, &a.lease, request)
        .await
        .expect("the same report replays");
    assert!(replay.settled && replay.replayed);
    assert_eq!(balance(&f).await, before - RUNTIME_DEBIT, "charged once");
    assert_eq!(f.counts().await, (0, 0, 0), "no private row came back");
    assert_eq!(provider.count(), 1);
    f.cleanup().await;
}

/// The runner never reported: its lease lapsed and the closed conversation
/// passed its private retention, so maintenance owns both.
async fn expire_without_a_report(f: &Fixture, a: &Attempt) {
    let mut connection = f.connection().await;
    sqlx::query("UPDATE authoring.attempts SET lease_expires_at='epoch' WHERE id=$1::uuid")
        .bind(a.id.as_str())
        .execute(&mut *connection)
        .await
        .expect("the lease lapses");
    sqlx::query("UPDATE authoring.drafts SET status='closed',expires_at='epoch' WHERE id=$1::uuid")
        .bind(a.draft.id.as_str())
        .execute(&mut *connection)
        .await
        .expect("private retention ends");
}

async fn attempt_status(f: &Fixture, a: &Attempt) -> String {
    sqlx::query_scalar("SELECT status FROM authoring.attempts WHERE id=$1::uuid")
        .bind(a.id.as_str())
        .fetch_one(&mut *f.connection().await)
        .await
        .expect("the accounting obligation outlives private rows")
}
