//! Invalid interpretations leave no partial task, receipt or continuation.

use std::ops::Range;

use afd_admission::Producer;
use afd_core::error_code::{self, ErrorCode};
use afd_fleet::task::{PreparedPlan, PreparedTask, TaskLifecycle as _};
use afd_wire::task::TaskLimits;

use super::fixture::{Case, GOALS, exercise, id, items, questions};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn only_current_claimed_human_intent_can_allocate_work() {
    exercise(source_authority).await;
}

async fn source_authority(case: &Case) {
    let tasks = &case.tasks;
    let missing = case.source_plan(
        id(&afd_db::test_util::mint_id()),
        "missing",
        1,
        TaskLimits::default(),
    );
    refused(case, &missing, error_code::TASK_NOT_FOUND).await;
    let human = afd_events::steer_actor("native_task_owner");
    let source = case.source(Producer::Steer, &human).await;
    let unclaimed = case.source_plan(source.clone(), "unclaimed", 2, TaskLimits::default());
    refused(case, &unclaimed, error_code::TASK_STALE_STATE).await;
    for (producer, actor) in [
        (Producer::Webhook, human.as_str()),
        (Producer::Steer, "service:automation"),
        (Producer::Steer, afd_events::ACTOR_MACHINE),
    ] {
        let source = case.source(producer, actor).await;
        case.claim(&source).await;
        let plan = case.source_plan(source, "not-human-intent", 2, TaskLimits::default());
        refused(case, &plan, error_code::TASK_INVALID_TARGET).await;
    }
    case.claim(&source).await;
    let bound = tasks
        .bind(case.lease(), &unclaimed)
        .await
        .expect("claimed human intent remains usable");
    assert_eq!(
        (bound.items.len(), bound.replayed, case.counts().await),
        (2, false, (2, 2, 6))
    );
    let replay = tasks
        .bind(case.lease(), &unclaimed)
        .await
        .expect("stable source replay");
    assert_eq!(replay.items, bound.items);
    assert!(replay.replayed);
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn malformed_spans_and_unapproved_limits_cannot_leave_a_partial_plan() {
    exercise(interpretation_bounds).await;
}

async fn interpretation_bounds(case: &Case) {
    let tasks = &case.tasks;
    for (span, expected) in [
        (Range { start: -1, end: 4 }, error_code::INTERNAL_DB_QUERY),
        (Range { start: 0, end: -1 }, error_code::INTERNAL_DB_QUERY),
        (0..i32::MAX, error_code::TASK_REPLAY_CONFLICT),
        (Range { start: 8, end: 2 }, error_code::TASK_REPLAY_CONFLICT),
    ] {
        let plan = span_plan(case, span);
        refused(case, &plan, expected).await;
    }
    for limits in lower_limits().into_iter().chain(upper_limits()) {
        let plan = case.source_plan(id(&case.held.event_id), "invalid-allowance", 3, limits);
        refused(case, &plan, error_code::TASK_LIMIT_EXHAUSTED).await;
    }
    let accepted = tasks
        .bind(case.lease(), &case.plan("corrected-interpretation", 3))
        .await
        .expect("valid retry");
    assert_eq!(
        (accepted.items.len(), accepted.replayed, case.counts().await),
        (3, false, (3, 3, 3))
    );
}

fn span_plan(case: &Case, span: Range<i32>) -> PreparedPlan {
    let item = PreparedTask::from_parts(
        span,
        GOALS[0].to_owned(),
        GOALS[0].to_owned(),
        questions(),
        "reviewed-goal".to_owned(),
        serde_json::json!({"kind":"api_only"}).to_string(),
        TaskLimits::default(),
    );
    PreparedPlan::from_parts(
        id(&case.held.event_id),
        case.revision.clone(),
        "invalid-span".to_owned(),
        vec![item],
    )
}

fn lower_limits() -> [TaskLimits; 7] {
    let approved = TaskLimits::default();
    [
        TaskLimits {
            model_nanos: 0,
            ..approved
        },
        TaskLimits {
            wall_seconds: 0,
            ..approved
        },
        TaskLimits {
            running_seconds: 0,
            ..approved
        },
        TaskLimits {
            leases: 0,
            ..approved
        },
        TaskLimits {
            publications: 0,
            ..approved
        },
        TaskLimits {
            decision_calls: 0,
            ..approved
        },
        TaskLimits {
            fallback_reads: 0,
            ..approved
        },
    ]
}

fn upper_limits() -> [TaskLimits; 7] {
    let approved = TaskLimits::default();
    [
        TaskLimits {
            model_nanos: approved.model_nanos + 1,
            ..approved
        },
        TaskLimits {
            wall_seconds: approved.wall_seconds + 1,
            ..approved
        },
        TaskLimits {
            running_seconds: approved.running_seconds + 1,
            ..approved
        },
        TaskLimits {
            leases: approved.leases + 1,
            ..approved
        },
        TaskLimits {
            publications: approved.publications + 1,
            ..approved
        },
        TaskLimits {
            decision_calls: approved.decision_calls + 1,
            ..approved
        },
        TaskLimits {
            fallback_reads: approved.fallback_reads + 1,
            ..approved
        },
    ]
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_new_interpretation_requires_the_current_saved_setup_revision() {
    exercise(revision).await;
}

async fn revision(case: &Case) {
    case.held
        .fixtures
        .native_config(
            &case.held.fleet,
            r#"{"name":"revised-repair-criteria","x-agentsfleet":{"tools":[],"triggers":[],"budget":{"daily_nanos":"1000000000"}}}"#,
        )
        .await;
    refused(
        case,
        &case.plan("old-setup", 3),
        error_code::TASK_STALE_STATE,
    )
    .await;
    let (revision, number): (String, i64) =
        sqlx::query_as("SELECT f.head_revision_id::text,r.revision FROM fleet.fleets f JOIN fleet.revisions r ON r.id=f.head_revision_id WHERE f.id=$1::uuid")
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
            .expect("current saved revision");
    assert_ne!(revision, case.revision.as_str());
    let number = u64::try_from(number).expect("positive saved revision");
    assert_eq!(number, case.revision_number + 1);
    accepted_revision(case, &revision, number).await;
}

async fn accepted_revision(case: &Case, revision: &str, number: u64) {
    let (tasks, scope) = (&case.tasks, &case.scope);
    let plan = PreparedPlan::from_parts(
        id(&case.held.event_id),
        id(revision),
        "new-setup".to_owned(),
        items(3, TaskLimits::default()),
    );
    let accepted = tasks
        .bind(case.lease(), &plan)
        .await
        .expect("current reviewed setup binds");
    assert_eq!(
        (accepted.items.len(), accepted.replayed, case.counts().await),
        (3, false, (3, 3, 3))
    );
    for binding in &accepted.items {
        let task = tasks
            .read(
                scope,
                binding.task_id.as_ref().expect("accepted goal owns a task"),
            )
            .await
            .expect("saved accepted goal");
        assert_eq!(
            (
                task.context.accepted_revision_id.as_ref(),
                task.accepted_revision_number,
                task.context.generation,
                task.resource_version
            ),
            (revision, number, 1, 1)
        );
    }
}

async fn refused(case: &Case, plan: &PreparedPlan, expected: ErrorCode) {
    let (tasks, scope) = (&case.tasks, &case.scope);
    let before = case.counts().await;
    let refusal = tasks
        .bind(case.lease(), plan)
        .await
        .expect_err("native qualification rejects this complete plan");
    assert_eq!(refusal.code(), expected);
    assert!(
        !refusal.detail().contains(GOALS[0]),
        "public diagnostics contain no objective text"
    );
    assert_eq!(
        case.counts().await,
        before,
        "every rejected interpretation rolls back completely"
    );
    let page = tasks
        .list(scope, None, 16)
        .await
        .expect("unfinished work read");
    assert_eq!(page.items, Vec::new(), "no work survives refusal");
    assert!(!page.more);
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_current_lease_without_durable_capability_cannot_allocate_work() {
    exercise(async |case| {
        let plan = case.plan("capability", 1);
        super::fixture::task_capability(&case.held, false).await;
        refused(case, &plan, error_code::RUN_STALE_FENCING_TOKEN).await;
        super::fixture::task_capability(&case.held, true).await;
        case.tasks
            .bind(case.lease(), &plan)
            .await
            .expect("the same current lease binds after advertising protocol support");
    })
    .await;
}
