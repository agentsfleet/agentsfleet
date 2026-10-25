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
        let mut request = fixture::request(&attempt);
        request.request.supports_compaction = false;
        let result = calls
            .call(&f.runner, &attempt.lease, request)
            .await
            .expect("unchanged complete history can dispatch");
        assert!(!result.compacted, "{prior} previous exchanges");
        assert_eq!(result.text, "continued answer");
        assert_eq!(fixture::receipts(&f, &attempt).await, (1, 1, false));
        assert_eq!(provider.0.lock().expect("record requests").len(), 1);
        let checkpoints: i64 = sqlx::query_scalar(CHECKPOINT_COUNT)
            .bind(attempt.draft.id.as_str())
            .fetch_one(&mut *f.connection().await)
            .await
            .expect("working notes census");
        assert_eq!(checkpoints, 0);
        let messages: i64 =
            sqlx::query_scalar("SELECT count(*) FROM authoring.messages WHERE draft_id=$1::uuid")
                .bind(attempt.draft.id.as_str())
                .fetch_one(&mut *f.connection().await)
                .await
                .expect("original history census");
        assert_eq!(messages, prior + 1);
        f.cleanup().await;
    }
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn an_older_runner_cannot_pay_for_a_summary_it_cannot_resume() {
    let f = Fixture::create().await;
    sqlx::query("UPDATE model.platform_models SET context_cap_tokens=24000 WHERE id=$1::uuid")
        .bind(f.catalogue.first().expect("owned model").as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("force summary preparation");
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    history(&f, &attempt, 180).await;
    let provider = Arc::new(Provider::default());
    let calls = fixture::calls(&f, &attempt, Arc::clone(&provider)).await;
    let before = financial_state(&f).await;
    let mut request = fixture::request(&attempt);
    request.request.supports_compaction = false;

    let error = calls
        .call(&f.runner, &attempt.lease, request)
        .await
        .expect_err("the runner must opt in before a summary can be funded");

    assert_eq!(error.code(), afd_core::error_code::AUTHORING_TOKEN);
    assert_eq!(fixture::receipts(&f, &attempt).await, (0, 0, false));
    assert!(provider.0.lock().expect("record requests").is_empty());
    assert_eq!(financial_state(&f).await, before);
    let checkpoints: i64 = sqlx::query_scalar(CHECKPOINT_COUNT)
        .bind(attempt.draft.id.as_str())
        .fetch_one(&mut *f.connection().await)
        .await
        .expect("no unsupported summary");
    assert_eq!(checkpoints, 0);
    f.cleanup().await;
}

async fn financial_state(f: &Fixture) -> (i64, i64, i64) {
    sqlx::query_as("SELECT balance_nanos,(SELECT count(*) FROM billing.holds WHERE tenant_id=$1::uuid),(SELECT COALESCE(sum(captured_nanos),0)::bigint FROM billing.holds WHERE tenant_id=$1::uuid) FROM billing.wallets WHERE tenant_id=$1::uuid")
        .bind(f.tenant.as_str()).fetch_one(&mut *f.connection().await).await
        .expect("wallet balance, hold count and captured charges")
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn a_summary_receipt_cannot_be_replayed_without_the_original_runner_capability() {
    let f = Fixture::create().await;
    sqlx::query("UPDATE model.platform_models SET context_cap_tokens=24000 WHERE id=$1::uuid")
        .bind(f.catalogue.first().expect("owned model").as_str())
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
        .expect("new runner opts into a metered summary");
    assert!(result.compacted);
    let before = financial_state(&f).await;
    let mut downgraded = request.clone();
    downgraded.request.supports_compaction = false;
    let error = calls
        .call(&f.runner, &attempt.lease, downgraded)
        .await
        .expect_err("a changed capability is not the original request");
    assert_eq!(error.code(), afd_core::error_code::AUTHORING_REPLAY);
    assert_eq!(financial_state(&f).await, before);
    let replayed = calls
        .call(&f.runner, &attempt.lease, request)
        .await
        .expect("the exact capable request can recover its receipt");
    assert!(replayed.replayed && replayed.compacted);
    assert_eq!(provider.0.lock().expect("record requests").len(), 1);
    assert_eq!(fixture::receipts(&f, &attempt).await, (1, 1, false));
    f.cleanup().await;
}
