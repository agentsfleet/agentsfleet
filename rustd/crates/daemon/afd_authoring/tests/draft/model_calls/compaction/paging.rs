//! Full history pages must not look like concurrent edits during funded dispatch.
use super::{Attempt, CHECKPOINT_COUNT, Fixture, Provider, Source, fixture, history};
use std::sync::Arc;

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn complete_history_at_and_around_full_pages_dispatches_once_without_losing_messages() {
    for prior in [64, 128, 63, 65, 127, 129] {
        let f = Fixture::create().await;
        let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
        history(&f, &attempt, prior).await;
        let provider = Arc::new(Provider::default());
        let calls = fixture::calls(&f, &attempt, Arc::clone(&provider)).await;
        let request = fixture::request(&attempt);
        let result = calls
            .call(&f.runner, &attempt.lease, request)
            .await
            .expect("unchanged complete history can dispatch");
        assert!(!result.compacted, "{prior} previous exchanges");
        assert_eq!(result.text, "continued answer");
        assert_eq!(fixture::receipts(&f, &attempt).await, (1, 1, false));
        assert_eq!(provider.0.lock().expect("record requests").len(), 1);
        let checkpoints: i64 = sqlx::query_scalar(CHECKPOINT_COUNT)
            .bind(&attempt.draft.id)
            .fetch_one(&mut *f.connection().await)
            .await
            .expect("working notes census");
        assert_eq!(checkpoints, 0);
        let messages: i64 =
            sqlx::query_scalar("SELECT count(*) FROM authoring.messages WHERE draft_id=$1")
                .bind(attempt.draft.id)
                .fetch_one(&mut *f.connection().await)
                .await
                .expect("original history census");
        assert_eq!(messages, prior + 1);
        f.cleanup().await;
    }
}

async fn financial_state(f: &Fixture) -> (i64, i64, i64) {
    sqlx::query_as("SELECT balance_nanos,(SELECT count(*) FROM billing.usage_lines WHERE tenant_id=$1),(SELECT COALESCE(sum(charged_nanos),0)::bigint FROM billing.usage_lines WHERE tenant_id=$1) FROM billing.wallets WHERE tenant_id=$1")
        .bind(&f.tenant).fetch_one(&mut *f.connection().await).await
        .expect("wallet balance, line count and logged charges")
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_summary_receipt_cannot_be_replayed_by_a_changed_request() {
    let f = Fixture::create().await;
    sqlx::query("UPDATE model.platform_models SET context_cap_tokens=24000 WHERE id=$1")
        .bind(f.catalogue.first().expect("owned model"))
        .execute(&mut *f.connection().await)
        .await
        .expect("force compaction");
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    history(&f, &attempt, 180).await;
    let provider = Arc::new(Provider::default());
    let calls = fixture::calls(&f, &attempt, Arc::clone(&provider)).await;
    let request = fixture::request(&attempt);
    let result = calls
        .call(&f.runner, &attempt.lease, request.clone())
        .await
        .expect("the first request funds a metered summary");
    assert!(result.compacted);
    let before = financial_state(&f).await;
    let mut changed = request.clone();
    changed.request.max_output_tokens = request.request.max_output_tokens.saturating_sub(1).max(1);
    let error = calls
        .call(&f.runner, &attempt.lease, changed)
        .await
        .expect_err("a changed request is not the original request");
    assert_eq!(error.code(), af_core::error_code::AUTHORING_REPLAY);
    assert_eq!(financial_state(&f).await, before);
    let replayed = calls
        .call(&f.runner, &attempt.lease, request)
        .await
        .expect("the exact request can recover its receipt");
    assert!(replayed.replayed && replayed.compacted);
    assert_eq!(provider.0.lock().expect("record requests").len(), 1);
    assert_eq!(fixture::receipts(&f, &attempt).await, (1, 1, false));
    f.cleanup().await;
}
