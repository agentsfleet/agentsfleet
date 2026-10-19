//! Expiry closes the spinner while the original accounting obligation remains.

use super::{Attempt, Fixture};
use afd_authoring::types::{MessageQuery, Source};
use afd_wire::authoring::FailureReason;

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn an_expired_attempt_is_visible_as_waiting_for_accounting_without_freeing_its_bound() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    sqlx::query("UPDATE authoring.attempts SET reserved_nanos=30000000,lease_expires_at='epoch' WHERE id=$1::uuid")
        .bind(a.id.as_str()).execute(&mut *f.connection().await).await.expect("interrupted worker");
    f.drafts
        .maintain()
        .await
        .expect("production maintenance pass");
    let saved: (String, i64, i32) = sqlx::query_as(
        "SELECT status,reserved_nanos,call_count FROM authoring.attempts WHERE id=$1::uuid",
    )
    .bind(a.id.as_str())
    .fetch_one(&mut *f.connection().await)
    .await
    .expect("retained obligation");
    assert_eq!(saved, ("awaiting_accounting".into(), 30_000_000, 0));
    let messages = f
        .drafts
        .messages(
            f.access(),
            &a.draft.id,
            MessageQuery {
                limit: 20,
                starting_after: None,
            },
        )
        .await
        .expect("recover conversation");
    let answer = messages.items.last().expect("failure is visible");
    assert_eq!(answer.state.as_str(), "failed");
    assert_eq!(
        answer.failure.as_ref().expect("safe action").reason,
        FailureReason::ProviderUsageUnknown
    );
    assert!(answer.text.is_none());
    assert_eq!(a.activity_count(&f).await, 0);
    f.cleanup().await;
}

/// A maintenance pass expires every lane's stale attempts, so another lane's
/// pass may be the one that expires this attempt; the suite's shared recorder
/// sees its step either way, and the count is over every pass.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_expired_attempt_reports_its_message_finished_once_across_passes() {
    use super::super::funnel::{FINISHED, about, only, property};
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    sqlx::query("UPDATE authoring.attempts SET lease_expires_at='epoch' WHERE id=$1::uuid")
        .bind(a.id.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("interrupted worker");
    for _ in 0..2 {
        f.drafts
            .maintain()
            .await
            .expect("production maintenance pass");
    }
    let mine = Some(a.draft.id.as_str().to_owned());
    let finished: Vec<_> = f
        .reported(FINISHED)
        .into_iter()
        .filter(|event| property(event, "draft_id") == mine)
        .collect();
    let finished = only(finished, "the second pass finds nothing to expire");
    about(&f, &finished, &a.draft.id);
    assert_eq!(property(&finished, "state").as_deref(), Some("failed"));
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn another_workers_sweep_reports_to_the_same_workspace_recorder() {
    use super::super::funnel::{FINISHED, about, only};
    let f = Fixture::create().await;
    let worker = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    sqlx::query("UPDATE authoring.attempts SET lease_expires_at=to_timestamp(0) WHERE id=$1::uuid")
        .bind(a.id.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("interrupted worker");
    worker
        .drafts
        .maintain()
        .await
        .expect("another worker sweeps the shared store");
    let finished = only(
        f.reported(FINISHED),
        "the workspace sees the event whoever swept it",
    );
    about(&f, &finished, &a.draft.id);
    assert!(
        worker.reported(FINISHED).is_empty(),
        "another workspace cannot contaminate the assertion"
    );
    worker.cleanup().await;
    f.cleanup().await;
}

/// The attempt's stored status, and its message's.
async fn statuses(f: &Fixture, a: &Attempt) -> (String, String) {
    sqlx::query_as(
        "SELECT a.status, m.status FROM authoring.attempts a \
         JOIN authoring.messages m ON m.draft_id=a.draft_id AND m.sequence=a.message_sequence \
         WHERE a.id=$1::uuid",
    )
    .bind(a.id.as_str())
    .fetch_one(&mut *f.connection().await)
    .await
    .expect("the attempt and its message")
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn an_attempt_renewed_while_the_pass_waited_on_its_lock_is_left_running() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    sqlx::query("UPDATE authoring.attempts SET lease_expires_at='epoch' WHERE id=$1::uuid")
        .bind(a.id.as_str())
        .execute(&mut *f.connection().await)
        .await
        .expect("a lapsed lease the pass will find");
    let mut holder = f.connection().await;
    sqlx::query("BEGIN")
        .execute(&mut *holder)
        .await
        .expect("renewal transaction");
    let pid: i32 = sqlx::query_scalar(
        "UPDATE authoring.attempts SET lease_expires_at=now()+interval '1 minute' \
         WHERE id=$1::uuid RETURNING pg_backend_pid()",
    )
    .bind(a.id.as_str())
    .fetch_one(&mut *holder)
    .await
    .expect("a renewal holds the row");
    let (pass, ()) = tokio::join!(f.drafts.maintain(), async {
        f.waits_on(pid, 1).await;
        sqlx::query("COMMIT")
            .execute(&mut *holder)
            .await
            .expect("the renewal commits");
    });
    drop(holder);
    pass.expect("the pass runs");
    assert_eq!(
        statuses(&f, &a).await,
        ("active".to_owned(), "running".to_owned()),
        "rechecked under the lock, the renewed attempt is not expired"
    );
    f.cleanup().await;
}

#[tokio::test]
#[ignore = "requires live Postgres"]
async fn an_expired_attempt_whose_message_already_finished_keeps_that_finish() {
    let f = Fixture::create().await;
    let a = Attempt::start(&f, f.access(), Source::Blank).await;
    sqlx::query(
        "WITH stopped AS (UPDATE authoring.messages SET status='cancelled' WHERE draft_id=$2::uuid) \
         UPDATE authoring.attempts SET lease_expires_at='epoch' WHERE id=$1::uuid",
    )
    .bind(a.id.as_str())
    .bind(a.draft.id.as_str())
    .execute(&mut *f.connection().await)
    .await
    .expect("a stopped message on a lapsed lease");
    f.drafts.maintain().await.expect("the pass runs");
    assert_eq!(
        statuses(&f, &a).await,
        ("awaiting_accounting".to_owned(), "cancelled".to_owned()),
        "the money waits; the earlier finish stands"
    );
    f.cleanup().await;
}
