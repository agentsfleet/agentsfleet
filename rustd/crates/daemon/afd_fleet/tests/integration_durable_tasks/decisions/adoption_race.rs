//! Two deliveries of one adoption operation that race past the receipt read
//! still adopt once: the later one replays the receipt under the task lock.

use std::time::Duration;

use af_core::{error_code, id::Uuid7};
use af_wire::durable_task::DurableTaskState;
use afd_fleet::durable_task::{
    DurableTaskAdoption, DurableTaskLifecycle as _, DurableTaskSnapshot,
};
use futures_util::future::join3;
use serde_json::json;
use sqlx::Connection as _;

use super::super::fixture::{Case, exercise, id};
use super::fixture::{Decisions, lock_thread, policy};

/// How many of this case's own database backends wait on a lock. Each case owns
/// its database, so only its own adoptions are counted: the second can queue
/// behind the first rather than behind the held lock.
const WAITING: &str = "SELECT count(*) FROM pg_stat_activity WHERE datname = current_database() AND cardinality(pg_blocking_pids(pid)) > 0";

/// A reviewed head revision whose completion question differs from the bound one.
async fn revised(case: &Case) -> Uuid7 {
    let mut questions = policy();
    *questions
        .pointer_mut("/completion/statements/0/question")
        .expect("existing reviewed completion question") =
        json!("Does the title regression pass with the accepted repair?");
    let config = json!({"name":"pull-request-repair", "x-agentsfleet":{
        "tools":[], "triggers":[], "budget":{"daily_nanos":"1000000000"},
        "decision_policy":questions}});
    case.held
        .fixtures
        .native_config(&case.held.fleet, &config.to_string())
        .await;
    sqlx::query_scalar("SELECT head_revision_id FROM fleet.fleets WHERE id=$1")
        .bind(&case.held.fleet)
        .fetch_one(
            &mut *case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("revision connection"),
        )
        .await
        .expect("new reviewed revision")
}

/// Waits until `waiters` of the case's backends queue behind a lock.
async fn queued(case: &Case, waiters: i64) {
    let mut observer = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("lock observer");
    loop {
        let waiting: i64 = sqlx::query_scalar(WAITING)
            .fetch_one(&mut *observer)
            .await
            .expect("lock waits are readable");
        if waiting >= waiters {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// `first` then `second` adopt under one held thread lock, queued in that order.
async fn race(
    decisions: &Decisions<'_>,
    task: &Uuid7,
    first: (&str, &DurableTaskAdoption),
    second: (&str, &DurableTaskAdoption),
) -> (
    afd_fleet::Result<DurableTaskSnapshot>,
    afd_fleet::Result<DurableTaskSnapshot>,
) {
    let case = decisions.case;
    let mut holder = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("holder connection");
    let mut held = holder.begin().await.expect("holder transaction");
    lock_thread(&mut held, case).await;
    let tasks = &decisions.tasks;
    let (first, second, ()) = join3(
        tasks.adopt_goal(&case.scope, task, first.0, first.1),
        async {
            queued(case, 1).await;
            tasks
                .adopt_goal(&case.scope, task, second.0, second.1)
                .await
        },
        async {
            queued(case, 2).await;
            held.rollback().await.expect("the held lock is released");
        },
    )
    .await;
    (first, second)
}

async fn receipts(case: &Case, task: &Uuid7) -> (i64, i64) {
    sqlx::query_as(
        "SELECT (SELECT count(*) FROM thread.replay_receipts WHERE resource_id=$1 AND scope LIKE '%:adoption'), (SELECT count(*) FROM thread.turns WHERE durable_task_id=$1 AND durable_task_generation=2 AND settled_at IS NULL)",
    )
    .bind(task)
    .fetch_one(&mut *case.held.fixtures.database.acquire().await.expect("receipt observer"))
    .await
    .expect("committed adoption and continuation")
}

async fn setup(decisions: &Decisions<'_>) -> (Uuid7, DurableTaskSnapshot, DurableTaskAdoption) {
    let case = decisions.case;
    let task = id(&decisions.bind_goal().await.durable_task_id);
    let before = decisions
        .tasks
        .read(&case.scope, &task)
        .await
        .expect("original goal");
    let adoption = DurableTaskAdoption {
        goal_revision_id: revised(case).await,
        operation_id: afd_db::test_util::mint_id(),
        expected_generation: before.context.generation,
    };
    (task, before, adoption)
}

// Value: protects=a retried adoption that raced its original past the receipt
// read adopts once and answers the adopted task; fails_when=the in-lock replay
// check is dropped and the second delivery adopts again, queues a second
// continuation, or refuses; why_new=the in-lock replay arm had no case;
// seam=the production adoption over live Postgres, both deliveries queued behind
// one held thread lock.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_raced_retry_of_one_adoption_replays_it_under_the_durable_task_lock() {
    exercise(async |case| {
        let decisions = Decisions::create(case).await;
        let (task, before, adoption) = setup(&decisions).await;
        let tag = before.etag();

        let (first, second) = race(&decisions, &task, (&tag, &adoption), (&tag, &adoption)).await;

        let first = first.expect("the original adopts");
        let second = second.expect("the raced retry replays the receipt");
        assert_eq!(first.context.state, DurableTaskState::Ready);
        assert_eq!(first.context.generation, 2);
        assert_eq!(second, first, "the retry answers the adopted task");
        assert_eq!(
            receipts(case, &task).await,
            (1, 1),
            "one receipt, one continuation"
        );
        decisions
            .model
            .finish()
            .await
            .expect("decision model cleanup");
    })
    .await;
}

// Value: protects=one operation id cannot carry two different adoptions, even
// when the second races the first past the receipt read; fails_when=the in-lock
// digest comparison is dropped and the second request is checked as new;
// why_new=the in-lock conflict arm had no case; seam=the production adoption over
// live Postgres, both deliveries queued behind one held thread lock.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_raced_reuse_of_one_operation_id_with_another_tag_is_a_replay_conflict() {
    exercise(async |case| {
        let decisions = Decisions::create(case).await;
        let (task, before, adoption) = setup(&decisions).await;
        let tag = before.etag();

        let (first, second) = race(
            &decisions,
            &task,
            (&tag, &adoption),
            ("another-tag", &adoption),
        )
        .await;

        let first = first.expect("the original adopts");
        let conflict = second.expect_err("another request under the same operation is refused");
        assert_eq!(conflict.code(), error_code::DURABLE_TASK_REPLAY_CONFLICT);
        assert_eq!(conflict.durable_task_current_state(), Some("ready"));
        assert_eq!(
            decisions
                .tasks
                .read(&case.scope, &task)
                .await
                .expect("adopted task"),
            first,
            "the refusal changed nothing"
        );
        assert_eq!(
            receipts(case, &task).await,
            (1, 1),
            "one receipt, one continuation"
        );
        decisions
            .model
            .finish()
            .await
            .expect("decision model cleanup");
    })
    .await;
}
