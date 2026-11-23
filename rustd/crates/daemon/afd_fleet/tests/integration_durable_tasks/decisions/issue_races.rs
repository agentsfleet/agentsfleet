//! A reservation's issue statement against a writer that holds a row it locks. The reads
//! before it hold nothing, so the reservation reaches the statement, queues there, and
//! re-checks the row once the writer ends: a takeover, a task cancel, a late receipt of a
//! retried call and a fleet delete that commit leave no call, and a lost usage number is
//! drawn again.

use af_core::{error_code, id::Uuid7, test_util::trace::Capture};
use af_wire::durable_task::{
    DecisionAssessment, DecisionCallState, DecisionReceipt, DecisionRetryDelay,
    DecisionUnavailableCause, DurableTaskState,
};
use afd_db::Db;
use afd_fleet::durable_task::DurableTaskLifecycle as _;
use sqlx::Connection as _;

use super::{
    super::fixture::{Case, exercise, id},
    fixture::{Decisions, lock_thread},
};

/// Whether an issue statement is waiting on a lock `$1` holds. The statement is named by
/// how `sql/issue.sql` opens: Postgres shows only a statement's first kilobyte.
const BLOCKING: &str = "SELECT EXISTS(SELECT 1 FROM pg_stat_activity \
    WHERE wait_event_type = 'Lock' AND query LIKE '-- A decision reservation''s one locking statement%' \
      AND $1 = ANY(pg_blocking_pids(pid)))";

/// What a takeover writes on the thread: the next fence.
const NEXT_FENCE: &str = "UPDATE thread.threads SET fencing_token = fencing_token + 1, \
    updated_at = now() WHERE fleet_id = $1";

/// The three things a person's Cancel moves on the task (`durable_task/sql/control.sql`),
/// one at a time: the issue statement refuses on each alone.
const TASK_MOVES: [&str; 3] = [
    "UPDATE fleet.durable_tasks SET status = $2, updated_at = now() WHERE id = $1",
    "UPDATE fleet.durable_tasks SET generation = generation + 1, updated_at = now() \
     WHERE id = $1 AND $2::text IS NOT NULL",
    "UPDATE fleet.durable_tasks SET stop_generation = stop_generation + 1, updated_at = now() \
     WHERE id = $1 AND $2::text IS NOT NULL",
];

/// What a late provider receipt writes on the call it accounts (`decision/sql/record.sql`).
const RECEIPTED: &str = "UPDATE runner.decision_calls SET status = $2, \
    canonical_receipt_digest = 'late receipt', updated_at = now() WHERE id = $1";

/// Fleet delete's first hold (`afd_setup/src/creation/sql/lock.sql`).
const LOCK_FLEET: &str = "SELECT pg_backend_pid() FROM fleet.fleets WHERE id = $1 FOR UPDATE";

/// A call of the same run holding the usage number after `$1`'s, under its own operation
/// and assessment, as a sibling reservation of the run would insert it.
const NEXT_USAGE_NUMBER: &str = "INSERT INTO runner.decision_calls \
    SELECT (jsonb_populate_record(NULL::runner.decision_calls, to_jsonb(c) || jsonb_build_object( \
        'id', $2::uuid, 'operation_id', $3::uuid, 'assessment_key', $4::text, \
        'usage_sequence', c.usage_sequence + 1))).* \
    FROM runner.decision_calls c WHERE c.id = $1";

/// Whether `work` queues behind a lock `pid` holds before it finishes; `work` keeps
/// running either way, so a statement that never waits fails the test, not hangs it.
pub(super) async fn queues_behind<F: Future + Unpin>(
    database: &Db,
    pid: i32,
    work: &mut F,
) -> bool {
    let mut probe = database.acquire().await.expect("a connection");
    let waiter = async {
        loop {
            let queued: bool = sqlx::query_scalar(BLOCKING)
                .bind(pid)
                .fetch_one(&mut *probe)
                .await
                .expect("inspect the lock queue");
            if queued {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    };
    tokio::select! {
        _ = work => false,
        () = waiter => true,
    }
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_reservation_that_waited_behind_a_takeover_issues_nothing() {
    for commits in [true, false] {
        exercise(async |case| takeover(case, commits).await).await;
    }
}

async fn takeover(case: &Case, commits: bool) {
    let fixture = Decisions::create(case).await;
    let input = fixture.reservation(DecisionAssessment::Message, None);
    let database = &case.held.fixtures.database;
    let mut holder = database.acquire().await.expect("a connection");
    let mut sibling = holder.begin().await.expect("the takeover's transaction");
    let pid = lock_thread(&mut sibling, case).await;
    sqlx::query(NEXT_FENCE)
        .bind(&case.held.fleet)
        .execute(&mut *sibling)
        .await
        .expect("the takeover raises the fence");

    let mut reserving = Box::pin(fixture.tasks.reserve_decision(case.run(), &input));
    assert!(
        queues_behind(database, pid, &mut reserving).await,
        "the reads pass and the issue statement waits on the thread"
    );
    if commits {
        sibling.commit().await.expect("the takeover commits");
        let refused = reserving.await.expect_err("the fence moved in the wait");
        assert_eq!(refused.code(), error_code::RUN_STALE_FENCING_TOKEN);
        assert_eq!(fixture.counts().await, (0, 0, 0));
    } else {
        sibling.rollback().await.expect("the takeover rolls back");
        let issued = reserving.await.expect("nothing moved");
        assert!(issued.into_parts().0);
        assert_eq!(fixture.counts().await, (1, 1, 0));
    }
    fixture.model.finish().await.expect("model cleanup");
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_reservation_that_waited_behind_a_task_cancel_issues_nothing() {
    for moved in TASK_MOVES {
        exercise(async |case| task_moved(case, moved).await).await;
    }
}

async fn task_moved(case: &Case, moved: &'static str) {
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    let input = fixture.goal_reservation(&task);
    let database = &case.held.fixtures.database;
    let mut holder = database.acquire().await.expect("a connection");
    let mut sibling = holder.begin().await.expect("the Cancel's transaction");
    let pid = lock_thread(&mut sibling, case).await;
    sqlx::query(moved)
        .bind(id(&task.durable_task_id))
        .bind(DurableTaskState::Cancelled.as_str())
        .execute(&mut *sibling)
        .await
        .expect("the Cancel moves the task");

    let mut reserving = Box::pin(fixture.tasks.reserve_decision(case.run(), &input));
    assert!(
        queues_behind(database, pid, &mut reserving).await,
        "the issue statement waits on the thread the Cancel holds"
    );
    sibling.commit().await.expect("the Cancel commits");
    let refused = reserving.await.expect_err("the task moved in the wait");
    assert_eq!(
        refused.code(),
        error_code::DURABLE_TASK_STALE_STATE,
        "{moved}"
    );
    assert_eq!(fixture.counts().await, (0, 0, 0));
    fixture.model.finish().await.expect("model cleanup");
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_retry_whose_call_was_receipted_in_the_wait_issues_nothing() {
    exercise(async |case: &Case| {
        let fixture = Decisions::create(case).await;
        let input = fixture.reservation(DecisionAssessment::Message, None);
        let (issued, body) = fixture.reserve(&input).await;
        assert!(issued);
        let call = body
            .get("call_id")
            .and_then(serde_json::Value::as_str)
            .expect("the throttled call")
            .to_owned();
        let throttled = DecisionReceipt::Unknown {
            cause: DecisionUnavailableCause::ProviderStatus {
                status: 429,
                retry_delay: DecisionRetryDelay::Unspecified,
            },
        };
        assert_eq!(
            fixture.record(&call, throttled).await.state,
            DecisionCallState::Unknown
        );
        let mut retry = (*input).clone();
        retry.operation_id = afd_db::test_util::mint_id().into();
        retry.retry_of = Some(call.clone());
        let retry = garde::Unvalidated::new(retry)
            .validate()
            .expect("a bounded retry");

        let database = &case.held.fixtures.database;
        let mut holder = database.acquire().await.expect("a connection");
        let mut receipt = holder.begin().await.expect("the receipt's transaction");
        let pid = lock_thread(&mut receipt, case).await;
        sqlx::query(RECEIPTED)
            .bind(id(&call))
            .bind(DecisionCallState::Known.as_str())
            .execute(&mut *receipt)
            .await
            .expect("a late receipt makes the call known");

        let mut reserving = Box::pin(fixture.tasks.reserve_decision(case.run(), &retry));
        assert!(
            queues_behind(database, pid, &mut reserving).await,
            "the retry read its call as unknown and waits on the thread"
        );
        receipt.commit().await.expect("the receipt commits");
        let refused = reserving
            .await
            .expect_err("the retried call is no longer unknown");
        assert_eq!(refused.code(), error_code::DURABLE_TASK_STALE_STATE);
        assert_eq!(fixture.counts().await, (1, 1, 0), "no retry call is issued");
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_fleet_delete_and_a_decision_reservation_both_finish() {
    exercise(async |case: &Case| {
        let fixture = Decisions::create(case).await;
        let input = fixture.reservation(DecisionAssessment::Message, None);
        let database = &case.held.fixtures.database;
        let mut holder = database.acquire().await.expect("a connection");
        let mut deleting = holder.begin().await.expect("the delete's transaction");
        let pid: i32 = sqlx::query_scalar(LOCK_FLEET)
            .bind(&case.held.fleet)
            .fetch_one(&mut *deleting)
            .await
            .expect("the delete holds the fleet");

        let mut reserving = Box::pin(fixture.tasks.reserve_decision(case.run(), &input));
        assert!(
            queues_behind(database, pid, &mut reserving).await,
            "the issue statement waits at the fleet, its first lock"
        );
        for statement in [
            "DELETE FROM thread.runs WHERE fleet_id = $1",
            "DELETE FROM fleet.fleets WHERE id = $1",
        ] {
            sqlx::query(statement)
                .bind(&case.held.fleet)
                .execute(&mut *deleting)
                .await
                .expect("the delete waits on nothing the reservation holds");
        }
        deleting.commit().await.expect("the delete commits");
        let refused = reserving.await.expect_err("the fleet is gone");
        assert_eq!(refused.code(), error_code::RUN_NOT_FOUND);
        assert_eq!(fixture.counts().await, (0, 0, 0));
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
}

#[expect(
    clippy::future_not_send,
    reason = "a thread-local trace `Capture` lives across `.await`; one thread polls it"
)]
async fn lost_number(case: &Case) {
    let capture = Capture::install();
    let fixture = Decisions::create(case).await;
    let task = fixture.bind_goal().await;
    let (issued, first) = fixture.reserve(&fixture.goal_reservation(&task)).await;
    assert!(issued);
    let first = id(first
        .get("call_id")
        .and_then(serde_json::Value::as_str)
        .expect("the run's first call"));

    let database = &case.held.fixtures.database;
    let mut holder = database.acquire().await.expect("a connection");
    let mut sibling = holder.begin().await.expect("the sibling's transaction");
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *sibling)
        .await
        .expect("the sibling's backend");
    let ids: [Uuid7; 2] = [afd_db::test_util::mint_id(), afd_db::test_util::mint_id()];
    sqlx::query(NEXT_USAGE_NUMBER)
        .bind(&first)
        .bind(&ids[0])
        .bind(&ids[1])
        .bind(format!("sibling/{}", ids[0]))
        .execute(&mut *sibling)
        .await
        .expect("the sibling draws usage number 1 and stays open");

    let input = fixture.goal_reservation(&task);
    let mut reserving = Box::pin(fixture.tasks.reserve_decision(case.run(), &input));
    assert!(
        queues_behind(database, pid, &mut reserving).await,
        "the statement drew number 1 too and waits on the sibling's row"
    );
    sibling.commit().await.expect("the sibling commits");
    let (issued, body) = reserving
        .await
        .expect("the reservation issues")
        .into_parts();
    assert!(issued);
    let body: serde_json::Value = serde_json::from_str(&body).expect("a reservation answer");
    let number: i64 =
        sqlx::query_scalar("SELECT usage_sequence FROM runner.decision_calls WHERE id = $1")
            .bind(id(body
                .get("call_id")
                .and_then(serde_json::Value::as_str)
                .expect("the issued call")))
            .fetch_one(&mut *database.acquire().await.expect("a connection"))
            .await
            .expect("the issued call's usage number");
    assert_eq!(number, 2, "it takes the number after the sibling's");
    let retried = capture
        .events()
        .iter()
        .filter(|event| event.field("event") == Some("dt_decision_sequence_retried"))
        .count();
    assert_eq!(retried, 1, "the statement ran once more");
    fixture.model.finish().await.expect("model cleanup");
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_reservation_that_lost_its_usage_sequence_takes_the_next() {
    exercise(lost_number).await;
}
