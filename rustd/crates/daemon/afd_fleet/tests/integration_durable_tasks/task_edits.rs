//! A person edits a task's statements under the generation they read, and the
//! fleet's unfinished-task cap holds at the start.

use af_core::{error_code, id::Uuid7};
use afd_fleet::durable_task::{
    DurableTaskLifecycle as _, DurableTaskStatements, DurableTasks,
    policy::{DurableTaskPolicy, DurableTaskSettings},
};

use super::fixture::{Case, GOALS, exercise};
use super::task_tools::{goal_span, started, statement, task_start, valid};

/// The task's stored `(generation, goal_digest)`.
async fn goal(case: &Case, task: &Uuid7) -> (i64, String) {
    sqlx::query_as("SELECT generation, goal_digest FROM fleet.durable_tasks WHERE id = $1")
        .bind(task)
        .fetch_one(
            &mut *case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("a connection"),
        )
        .await
        .expect("the task's goal")
}

/// A person's edit to `questions` under `generation`.
fn edit(generation: u64, questions: &[&str]) -> DurableTaskStatements {
    DurableTaskStatements {
        questions: questions.iter().map(|id| statement(id)).collect(),
        expected_generation: generation,
    }
}

// Value: protects=a person's statement edit replaces the task's questions and
// goal digest and moves its generation on; fails_when=the edit keeps the old
// questions, the digest, or the generation; seam=`DurableTasks::edit_statements`
// over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_question_edit_bumps_the_generation() {
    exercise(async |case| {
        let task = started(case).await;
        let before = case
            .tasks
            .read(&case.scope, &task)
            .await
            .expect("the task reads");
        let (generation, digest) = goal(case, &task).await;

        let after = case
            .tasks
            .edit_statements(
                &case.scope,
                &task,
                &before.etag(),
                &edit(1, &["fixed", "documented"]),
            )
            .await
            .expect("the person's edit is applied");

        let names: Vec<&str> = after
            .context
            .questions
            .iter()
            .map(|q| q.name.as_str())
            .collect();
        assert_eq!(names, ["goal.fixed", "goal.documented"]);
        let (moved, changed) = goal(case, &task).await;
        assert_eq!(moved, generation + 1);
        assert_ne!(changed, digest);
    })
    .await;
}

// Value: protects=an edit under a generation the task has moved past changes
// nothing; fails_when=a stale edit overwrites a newer goal; seam=
// `DurableTasks::edit_statements` over live Postgres; no_claim=the API key's
// refusal, which the route answers before this verb.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_stale_question_edit_changes_nothing() {
    exercise(async |case| {
        let task = started(case).await;
        let before = case
            .tasks
            .read(&case.scope, &task)
            .await
            .expect("the task reads");
        let stored = goal(case, &task).await;

        let stale = case
            .tasks
            .edit_statements(&case.scope, &task, &before.etag(), &edit(7, &["other"]))
            .await;

        assert_eq!(
            stale.err().map(|refusal| refusal.code()),
            Some(error_code::DURABLE_TASK_PRECONDITION_FAILED)
        );
        assert_eq!(goal(case, &task).await, stored, "the task is unchanged");
    })
    .await;
}

// Value: protects=the fleet's unfinished-task cap refuses one more start and
// writes nothing; fails_when=the cap is read after the insert, or not at all;
// seam=`DurableTasks::interpret` under a cap of one over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_task_start_past_the_unfinished_cap_is_refused() {
    exercise(async |case| {
        let settings = DurableTaskSettings {
            max_unfinished_per_fleet: 1,
            ..DurableTaskSettings::default()
        };
        let mut dependencies =
            afd_fleet::durable_task::DurableTaskDependencies::from(&case.held.fixtures.plane());
        dependencies.policy = DurableTaskPolicy::new(
            &garde::Unvalidated::new(settings)
                .validate()
                .expect("bounded settings"),
        );
        let capped = DurableTasks::new(case.held.runs.clone(), case.turns.clone(), dependencies);
        started(case).await;
        let source = case.assigned_source().await;
        let input = task_start(
            case,
            &source,
            goal_span(1),
            GOALS[1],
            vec![statement("second")],
        );

        let refused = capped.interpret(case.run(), &valid(input)).await;

        assert_eq!(
            refused.err().map(|refusal| refusal.code()),
            Some(error_code::DURABLE_TASK_LIMIT_EXHAUSTED)
        );
        assert_eq!(case.counts().await.0, 1, "no second task");
    })
    .await;
}
