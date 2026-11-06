//! Reviewed goal adoption preserves custody, cumulative limits and replay identity.

use afd_core::{error_code, id::Uuid7};
use afd_fleet::task::{TaskAdoption, TaskControl, TaskLifecycle as _, TaskSnapshot};
use afd_wire::task::TaskState;
use serde_json::json;

use super::super::fixture::{Case, exercise, id};
use super::fixture::{Decisions, policy};

async fn revised(case: &Case, daily: i64) -> Uuid7 {
    let mut questions = policy();
    questions["completion"]["statements"][0]["question"] =
        json!("Does the title regression pass with the accepted repair?");
    let config = json!({"name":"pull-request-repair", "x-agentsfleet":{
        "tools":[], "triggers":[], "budget":{"daily_nanos":daily.to_string()},
        "decision_policy":questions}});
    case.held
        .fixtures
        .native_config(&case.held.fleet, &config.to_string())
        .await;
    let revision: String =
        sqlx::query_scalar("SELECT head_revision_id::text FROM fleet.fleets WHERE id=$1::uuid")
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
            .expect("new reviewed revision");
    id(&revision)
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn reviewed_goal_adoption_fences_old_execution_and_replays_without_another_wake() {
    exercise(async |case| {
        let decisions = Decisions::create(case).await;
        let version = decisions.bind_goal().await;
        let task = id(&version.task_id);
        let before = decisions
            .tasks
            .read(&case.scope, &task)
            .await
            .expect("original goal");
        let revision = revised(case, 1_000_000_000).await;
        let adoption = TaskAdoption {
            goal_revision_id: revision.clone(),
            operation_id: id(&afd_db::test_util::mint_id()),
            expected_generation: before.context.generation,
        };
        let after = decisions
            .tasks
            .adopt_goal(&case.scope, &task, &before.etag(), &adoption)
            .await
            .expect("explicit reviewed adoption");
        assert_adopted(&before, &after, &revision);
        assert_eq!(case.counts().await, (1, 1, 2));
        replay_adoption(&decisions, &task, &before, &after, &adoption).await;
        decisions
            .model
            .finish()
            .await
            .expect("decision model cleanup");
    })
    .await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn paused_goal_adoption_preserves_pause_and_same_revision_is_an_idempotent_receipt() {
    exercise(async |case| {
        let decisions = Decisions::create(case).await;
        let version = decisions.bind_goal().await;
        let task = id(&version.task_id);
        let initial = decisions
            .tasks
            .read(&case.scope, &task)
            .await
            .expect("initial task");
        let paused = decisions
            .tasks
            .control(&case.scope, &task, &initial.etag(), 1, TaskControl::Pause)
            .await
            .expect("pause before adoption");
        let revision = revised(case, 1_000_000_000).await;
        let adoption = TaskAdoption {
            goal_revision_id: revision,
            operation_id: id(&afd_db::test_util::mint_id()),
            expected_generation: 2,
        };
        let adopted = decisions
            .tasks
            .adopt_goal(&case.scope, &task, &paused.etag(), &adoption)
            .await
            .expect("adopt without restarting paused work");
        assert_eq!(adopted.context.state, TaskState::Paused);
        assert_eq!(adopted.context.generation, 3);
        assert_eq!(adopted.context.stop_generation, 2);
        assert_eq!(case.counts().await, (1, 1, 1));
        same_revision(&decisions, &task, &adopted, adoption).await;
        decisions
            .model
            .finish()
            .await
            .expect("decision model cleanup");
    })
    .await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn adoption_refuses_stale_versions_unreviewed_goals_and_changed_authority() {
    exercise(async |case| {
        let decisions = Decisions::create(case).await;
        let version = decisions.bind_goal().await;
        let task = id(&version.task_id);
        let before = decisions
            .tasks
            .read(&case.scope, &task)
            .await
            .expect("original goal");
        let revision = revised(case, 2_000_000_000).await;
        let adoption = TaskAdoption {
            goal_revision_id: revision,
            operation_id: id(&afd_db::test_util::mint_id()),
            expected_generation: 1,
        };
        stale_adoption(&decisions, &task, &before, &adoption).await;
        refused_authority(&decisions, &task, &before, adoption).await;
        decisions
            .model
            .finish()
            .await
            .expect("decision model cleanup");
    })
    .await;
}

fn assert_adopted(before: &TaskSnapshot, after: &TaskSnapshot, revision: &Uuid7) {
    assert_eq!(after.context.state, TaskState::Ready);
    assert_eq!(after.context.generation, 2);
    assert_eq!(after.context.stop_generation, 1);
    assert_eq!(
        after.context.accepted_revision_id.as_ref(),
        revision.as_str()
    );
    assert_eq!(after.context.objective, before.context.objective);
    assert_eq!(after.context.usage, before.context.usage);
    assert_eq!(after.context.limits, before.context.limits);
    assert_eq!(after.context.expires_at, before.context.expires_at);
    assert_ne!(after.context.questions, before.context.questions);
    assert_ne!(after.context.goal_digest, before.context.goal_digest);
}

async fn assert_receipt(case: &Case, task: &Uuid7) {
    let stored: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM fleet.task_adoptions WHERE task_id=$1::uuid), (SELECT count(*) FROM fleet.admissions WHERE task_id=$1::uuid AND task_generation=2 AND settled_at IS NULL)",
    ).bind(task.as_str())
        .fetch_one(&mut *case.held.fixtures.database.acquire().await.expect("receipt connection"))
        .await.expect("committed adoption and continuation");
    assert_eq!(stored, (1, 1));
}

async fn replay_adoption(
    decisions: &Decisions<'_>,
    task: &Uuid7,
    before: &TaskSnapshot,
    after: &TaskSnapshot,
    adoption: &TaskAdoption,
) {
    let case = decisions.case;
    assert_eq!(
        decisions
            .tasks
            .adopt_goal(&case.scope, task, &before.etag(), adoption)
            .await
            .expect("lost response replays original operation"),
        *after
    );
    assert_eq!(case.counts().await, (1, 1, 2));
    let conflict = decisions
        .tasks
        .adopt_goal(&case.scope, task, &after.etag(), adoption)
        .await
        .expect_err("operation cannot change its original resource tag");
    assert_eq!(conflict.code(), error_code::TASK_REPLAY_CONFLICT);
    assert_eq!(conflict.task_current_state(), Some("ready"));
    assert_receipt(case, task).await;
}

async fn same_revision(
    decisions: &Decisions<'_>,
    task: &Uuid7,
    adopted: &TaskSnapshot,
    adoption: TaskAdoption,
) {
    let case = decisions.case;
    let same = TaskAdoption {
        expected_generation: 3,
        operation_id: id(&afd_db::test_util::mint_id()),
        ..adoption
    };
    assert_eq!(
        decisions
            .tasks
            .adopt_goal(&case.scope, task, &adopted.etag(), &same)
            .await
            .expect("same goal records receipt without transition"),
        *adopted
    );
    assert_eq!(case.counts().await, (1, 1, 1));
}

async fn refused_authority(
    decisions: &Decisions<'_>,
    task: &Uuid7,
    before: &TaskSnapshot,
    adoption: TaskAdoption,
) {
    let case = decisions.case;
    let authority = decisions
        .tasks
        .adopt_goal(&case.scope, task, &before.etag(), &adoption)
        .await
        .expect_err("goal adoption cannot widen budget authority");
    assert_eq!(authority.code(), error_code::TASK_INVALID_TARGET);
    assert_eq!(authority.task_current_state(), Some("running"));
    let unreviewed = TaskAdoption {
        goal_revision_id: decisions.revision.clone(),
        ..adoption
    };
    let old = decisions
        .tasks
        .adopt_goal(&case.scope, task, &before.etag(), &unreviewed)
        .await
        .expect_err("only current reviewed head can be adopted");
    assert_eq!(old.code(), error_code::TASK_INVALID_TARGET);
    assert_eq!(
        decisions
            .tasks
            .read(&case.scope, task)
            .await
            .expect("unchanged task"),
        *before
    );
    assert_eq!(case.counts().await, (1, 1, 1));
}

async fn stale_adoption(
    decisions: &Decisions<'_>,
    task: &Uuid7,
    before: &TaskSnapshot,
    adoption: &TaskAdoption,
) {
    let case = decisions.case;
    let stale = decisions
        .tasks
        .adopt_goal(&case.scope, task, "old-tag", adoption)
        .await
        .expect_err("stale tag rejected");
    assert_eq!(stale.code(), error_code::TASK_PRECONDITION_FAILED);
    assert_eq!(stale.task_precondition_tag(), Some(before.etag().as_str()));
    let wrong_generation = TaskAdoption {
        expected_generation: 2,
        ..adoption.clone()
    };
    let stale = decisions
        .tasks
        .adopt_goal(&case.scope, task, &before.etag(), &wrong_generation)
        .await
        .expect_err("stale generation rejected");
    assert_eq!(stale.code(), error_code::TASK_STALE_STATE);
    assert_eq!(stale.task_current_state(), Some("running"));
}
