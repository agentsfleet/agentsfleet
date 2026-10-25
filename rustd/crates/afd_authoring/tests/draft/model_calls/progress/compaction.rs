//! Publication status cannot outrun the durable checkpoint or escape a stopped lease.
use super::fixture::{Live, WAIT};
use crate::draft::{attempt::Attempt, fixture::Fixture, model_calls};
use afd_authoring::types::Source;
use afd_wire::context::ContextState;
use serde_json::{Value, json};

const SUMMARY: &str = r#"{"goal":"private summary goal","constraints":[],"decisions":[],"completed":[],"remaining":["continue"],"references":[]}"#;

mod content;
mod publication;

async fn ready() -> (Fixture, Attempt, Live) {
    ready_in(Fixture::create().await).await
}

async fn ready_in(f: Fixture) -> (Fixture, Attempt, Live) {
    sqlx::query("UPDATE model.platform_models SET context_cap_tokens=24000 WHERE id=$1::uuid")
        .bind(f.catalogue.first().expect("owned model").as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("force summary preparation");
    let attempt = Attempt::start(&f, f.access(), Source::Blank).await;
    model_calls::compaction::history(&f, &attempt, 180).await;
    let live = Live::start(&f, &attempt).await;
    (f, attempt, live)
}

async fn finish(live: &Live, text: &str) {
    live.text(text).await;
    let response = json!({"status":"completed","output":[{"type":"message",
        "content":[{"type":"output_text","text":text}]}],"usage":{"input_tokens":100,"output_tokens":1}});
    live.frame(json!({"type":"response.completed","response":response}))
        .await;
}

async fn checkpoint(f: &Fixture, attempt: &Attempt) -> Option<Value> {
    sqlx::query_scalar("SELECT document FROM memory.context_checkpoints WHERE draft_id=$1::uuid")
        .bind(attempt.draft.id.as_str())
        .fetch_optional(&mut *f.connection().await)
        .await
        .expect("observe only committed notes")
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires live Postgres and Dragonfly: make test-integration-rustd"]
async fn summary_progress_resumes_only_after_a_checkpoint_is_readable() {
    let (f, attempt, mut live) = ready().await;
    let request = model_calls::fixture::request(&attempt);
    let task = live.call(&f.runner, &attempt, request.clone());
    live.arrived().await;
    let started = live.context().await;
    assert_eq!(started.progress.state, ContextState::Summarizing);
    assert_eq!(started.progress.generation, 1);
    assert_eq!(started.sequence, "181");
    assert_eq!(started.stream_id.as_str(), request.dispatch_id);
    assert!(!task.is_finished());
    assert!(checkpoint(&f, &attempt).await.is_none());
    finish(&live, SUMMARY).await;
    let ended = live.context().await;
    assert_eq!(ended.progress.state, ContextState::Resuming);
    assert_eq!(ended.stream_id, started.stream_id);
    assert_eq!(ended.progress.generation, 1);
    let saved = checkpoint(&f, &attempt)
        .await
        .expect("resuming requires committed notes");
    assert_eq!(saved.get("generation"), Some(&json!(1)));
    assert_eq!(
        saved.pointer("/summary/goal"),
        Some(&json!("private summary goal"))
    );
    let result = tokio::time::timeout(WAIT, task)
        .await
        .expect("summary receipt")
        .expect("dispatch task")
        .expect("known summary");
    assert!(result.compacted && result.text.is_empty());
    assert_eq!(
        model_calls::fixture::receipts(&f, &attempt).await,
        (1, 1, false)
    );
    live.drained().await;
    drop(live.reader);
    f.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires live Postgres and Dragonfly: make test-integration-rustd"]
async fn unusable_summary_reports_interruption_and_retains_known_usage_without_a_checkpoint() {
    let (f, attempt, mut live) = ready().await;
    let task = live.call(&f.runner, &attempt, model_calls::fixture::request(&attempt));
    live.arrived().await;
    assert_eq!(
        live.context().await.progress.state,
        ContextState::Summarizing
    );
    finish(&live, "unusable private notes").await;
    assert_eq!(
        live.context().await.progress.state,
        ContextState::Interrupted
    );
    let error = tokio::time::timeout(WAIT, task)
        .await
        .expect("summary receipt")
        .expect("dispatch task")
        .expect_err("unusable notes");
    assert_eq!(error.code(), afd_core::error_code::AUTHORING_PROVIDER);
    assert!(checkpoint(&f, &attempt).await.is_none());
    assert_eq!(
        model_calls::fixture::receipts(&f, &attempt).await,
        (1, 1, false)
    );
    live.drained().await;
    drop(live.reader);
    f.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires live Postgres and Dragonfly: make test-integration-rustd"]
async fn stop_withholds_the_summary_ending_while_its_known_usage_still_settles() {
    let (f, attempt, mut live) = ready().await;
    let task = live.call(&f.runner, &attempt, model_calls::fixture::request(&attempt));
    live.arrived().await;
    assert_eq!(
        live.context().await.progress.state,
        ContextState::Summarizing
    );
    f.drafts
        .cancel(f.access(), &attempt.draft.id, 181)
        .await
        .expect("stop current message");
    finish(&live, SUMMARY).await;
    let error = tokio::time::timeout(WAIT, task)
        .await
        .expect("late summary settlement")
        .expect("dispatch task")
        .expect_err("stopped publication");
    assert_eq!(error.code(), afd_core::error_code::AUTHORING_FENCE);
    assert!(checkpoint(&f, &attempt).await.is_none());
    assert_eq!(
        model_calls::fixture::receipts(&f, &attempt).await,
        (1, 1, false)
    );
    live.drained().await;
    drop(live.reader);
    f.cleanup().await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "requires live Postgres and Dragonfly: make test-integration-rustd"]
async fn changed_or_removed_history_withholds_late_notes_but_settles_known_usage() {
    for remove in [false, true] {
        let (f, attempt, mut live) = ready().await;
        let task = live.call(&f.runner, &attempt, model_calls::fixture::request(&attempt));
        live.arrived().await;
        assert_eq!(
            live.context().await.progress.state,
            ContextState::Summarizing
        );
        let mutation = if remove {
            "DELETE FROM authoring.messages WHERE draft_id=$1::uuid AND sequence < 181"
        } else {
            "UPDATE authoring.messages SET answer_text='changed-private-history' WHERE draft_id=$1::uuid AND sequence=1"
        };
        sqlx::query(mutation)
            .bind(attempt.draft.id.as_str())
            .execute(&mut *f.connection().await)
            .await
            .expect("change only this private history while its model is waiting");
        finish(&live, SUMMARY).await;
        assert_eq!(
            live.context().await.progress.state,
            ContextState::Interrupted
        );
        let error = tokio::time::timeout(WAIT, task)
            .await
            .expect("late history verification")
            .expect("dispatch task")
            .expect_err("stale summary cannot replace notes");
        assert_eq!(error.code(), afd_core::error_code::AUTHORING_FENCE);
        assert!(checkpoint(&f, &attempt).await.is_none());
        assert_eq!(
            model_calls::fixture::receipts(&f, &attempt).await,
            (1, 1, false)
        );
        live.drained().await;
        drop(live.reader);
        f.cleanup().await;
    }
}
