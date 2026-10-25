//! Conflicting notes and a real storage failure cannot partially publish a summary.
use super::{SUMMARY, WAIT, checkpoint, finish, model_calls, ready, ready_in};
use crate::draft::fixture::Fixture;
use afd_core::error_code;
use afd_wire::context::ContextState;
use model_calls::compaction::competing;
use sqlx::Connection as _;

#[tokio::test]
#[ignore = "requires live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_competing_checkpoint_withholds_late_notes_but_settles_known_usage() {
    let (f, attempt, mut live) = ready().await;
    let task = live.call(&f.runner, &attempt, model_calls::fixture::request(&attempt));
    live.arrived().await;
    assert_eq!(
        live.context().await.progress.state,
        ContextState::Summarizing
    );
    let original = competing(&f, &attempt).await;
    finish(&live, SUMMARY).await;
    assert_eq!(
        live.context().await.progress.state,
        ContextState::Interrupted
    );
    let error = tokio::time::timeout(WAIT, task)
        .await
        .expect("known usage settles")
        .expect("dispatch task")
        .expect_err("another writer already published notes");
    assert_eq!(error.code(), error_code::AUTHORING_FENCE);
    assert_eq!(checkpoint(&f, &attempt).await, Some(original));
    assert_eq!(
        model_calls::fixture::receipts(&f, &attempt).await,
        (1, 1, false)
    );
    live.drained().await;
    drop(live.reader);
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres and Dragonfly: make test-integration-rustd"]
async fn a_failed_checkpoint_write_rolls_back_the_receipt_without_publishing_private_notes() {
    let (f, attempt, mut live) = ready_in(Fixture::isolated().await).await;
    let task = live.call(&f.runner, &attempt, model_calls::fixture::request(&attempt));
    live.arrived().await;
    assert_eq!(
        live.context().await.progress.state,
        ContextState::Summarizing
    );
    let mut owner = refuse_notes(&f).await;
    finish(&live, SUMMARY).await;
    assert_eq!(
        live.context().await.progress.state,
        ContextState::Interrupted
    );
    let error = tokio::time::timeout(WAIT, task)
        .await
        .expect("failed publication returns")
        .expect("dispatch task")
        .expect_err("checkpoint write was refused");
    assert_eq!(error.code(), error_code::INTERNAL_OPERATION_FAILED);
    assert!(!error.to_string().contains("private fixture fault"));
    assert!(checkpoint(&f, &attempt).await.is_none());
    assert_eq!(
        model_calls::fixture::receipts(&f, &attempt).await,
        (1, 1, false)
    );
    let pending: (String, bool) = sqlx::query_as("SELECT status,result IS NULL AND usage IS NULL FROM authoring.dispatches WHERE attempt_id=$1::uuid")
        .bind(attempt.id.as_str()).fetch_one(&mut *f.connection().await).await.expect("rolled back receipt");
    assert_eq!(pending, ("pending".into(), true));
    sqlx::query("DROP TRIGGER refuse_notes ON memory.context_checkpoints")
        .execute(&mut owner)
        .await
        .expect("remove fault");
    owner.close().await.expect("close fault session");
    live.drained().await;
    drop(live.reader);
    f.cleanup().await;
}

async fn refuse_notes(f: &Fixture) -> sqlx::PgConnection {
    let mut owner = sqlx::PgConnection::connect(&f.owner_url())
        .await
        .expect("isolated fault owner");
    for sql in [
        "CREATE FUNCTION public.refuse_notes() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'private fixture fault'; END $$",
        "CREATE TRIGGER refuse_notes BEFORE INSERT ON memory.context_checkpoints FOR EACH ROW EXECUTE FUNCTION public.refuse_notes()",
    ] {
        sqlx::query(sql)
            .execute(&mut owner)
            .await
            .expect("install the real checkpoint fault");
    }
    owner
}
