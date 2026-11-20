//! Deployment limits govern new work without rewriting accepted receipts or allowances.

use af_core::error_code;
use af_wire::task::{TaskInputAction, TaskLimits, TaskRequestedTarget};
use afd_fleet::task::{
    TaskLifecycle as _, Tasks,
    policy::{TaskDefaults, TaskPolicy, TaskSettings},
};

use super::{Case, Decisions, digest, exercise, proposed, valid};

fn configured(fixture: &Decisions<'_>, settings: TaskSettings) -> Tasks {
    let checked = garde::Unvalidated::new(settings)
        .validate()
        .expect("safe deployment policy");
    let mut dependencies = fixture.dependencies.clone();
    dependencies.policy = TaskPolicy::new(&checked);
    Tasks::new(
        fixture.case.held.runs.clone(),
        fixture.case.turns.clone(),
        dependencies,
    )
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn custom_allowances_are_saved_and_replay_survives_narrower_admission_limits() {
    exercise(saved_limits).await;
}

async fn saved_limits(case: &Case) {
    let fixture = Decisions::create(case).await;
    let tasks = configured(
        &fixture,
        TaskSettings {
            defaults: TaskDefaults {
                model_budget_nanodollars: 750_000_000,
                wall_seconds: 900,
                running_seconds: 600,
                max_runs: 3,
                max_publication_attempts: 2,
                max_decision_calls: 9,
                max_fallback_reads: 3,
            },
            ..Default::default()
        },
    );
    let input = valid(proposed(&fixture, TaskInputAction::NewWork).await);
    let accepted = tasks
        .interpret(case.run(), &input)
        .await
        .expect("new objective");
    assert!(!accepted.replayed);
    let context = accepted.current_task.as_ref().expect("active new task");
    let expected = TaskLimits {
        model_nanos: 750_000_000,
        wall_seconds: 900,
        running_seconds: 600,
        runs: 3,
        publications: 2,
        decision_calls: 9,
        fallback_reads: 3,
    };
    assert_eq!(context.limits, expected);
    narrower_replay(case, &fixture, &input, &accepted, expected).await;
    fixture.model.finish().await.expect("model cleanup");
}

async fn narrower_replay(
    case: &Case,
    fixture: &Decisions<'_>,
    input: &garde::Valid<af_wire::task::TaskBindingsInput>,
    accepted: &af_wire::task::TaskBindingsResponse<'_>,
    expected: TaskLimits,
) {
    let context = accepted.current_task.as_ref().expect("active saved task");
    let task =
        af_core::id::Uuid7::parse(&context.task_id).expect("accepted native task identifier");
    let narrowed = configured(
        fixture,
        TaskSettings {
            max_title_bytes: 1,
            max_objective_bytes: 1,
            max_completion_questions: 1,
            ..Default::default()
        },
    );
    let replay = narrowed
        .interpret(case.run(), input)
        .await
        .expect("committed replay");
    assert!(replay.replayed);
    assert_eq!(replay.items, accepted.items);
    let stored = narrowed
        .read(&case.scope, &task)
        .await
        .expect("saved allowances");
    assert_eq!(stored.context.limits, expected);
    assert_eq!(case.counts().await, (1, 1, 1));
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn admission_bounds_refuse_the_whole_interpretation_before_target_reads() {
    exercise(refused_bounds).await;
}

async fn refused_bounds(case: &Case) {
    let fixture = Decisions::create(case).await;
    let mut input = proposed(&fixture, TaskInputAction::NewWork).await;
    input
        .items
        .first_mut()
        .expect("new objective")
        .proposed
        .target = Some(TaskRequestedTarget::ExistingPr {
        repository: "example/linkwarden".to_owned(),
        number: 101,
    });
    digest(&mut input);
    let input = valid(input);
    let before = case.counts().await;
    let policies = [
        TaskSettings {
            max_title_bytes: 1,
            ..Default::default()
        },
        TaskSettings {
            max_objective_bytes: 5,
            ..Default::default()
        },
        TaskSettings {
            max_completion_questions: 1,
            ..Default::default()
        },
        TaskSettings {
            max_question_text_bytes: 1,
            ..Default::default()
        },
        TaskSettings {
            max_authored_question_bytes: 1,
            ..Default::default()
        },
    ];
    for policy in policies {
        let failure = configured(&fixture, policy)
            .interpret(case.run(), &input)
            .await
            .expect_err("policy refuses before unconfigured remote storage");
        assert_eq!(failure.code(), error_code::TASK_LIMIT_EXHAUSTED);
        assert_eq!(case.counts().await, before);
    }
    fixture.model.finish().await.expect("model cleanup");
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_smaller_action_limit_refuses_the_complete_message() {
    exercise(async |case| {
        let fixture = Decisions::create(case).await;
        let tasks = configured(
            &fixture,
            TaskSettings {
                max_actions_per_message: 1,
                ..Default::default()
            },
        );
        let mut input = proposed(&fixture, TaskInputAction::Conversation).await;
        let mut second = input.items.first().expect("first action").clone();
        second.proposed.source_start = 6;
        second.proposed.source_end = 12;
        input.items.push(second);
        digest(&mut input);
        let before = case.counts().await;
        let failure = tasks
            .interpret(case.run(), &valid(input))
            .await
            .expect_err("whole message cap");
        assert_eq!(failure.code(), error_code::TASK_LIMIT_EXHAUSTED);
        assert_eq!(case.counts().await, before);
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn the_saved_fleet_thread_serializes_the_configured_unfinished_limit() {
    exercise(async |case| {
        let settings = garde::Unvalidated::new(TaskSettings {
            max_unfinished_per_fleet: 1,
            ..Default::default()
        })
        .validate()
        .expect("one-task admission policy");
        let mut dependencies = afd_fleet::task::TaskDependencies::from(&case.held.fixtures.plane());
        dependencies.policy = TaskPolicy::new(&settings);
        let tasks = Tasks::new(case.held.runs.clone(), case.turns.clone(), dependencies);
        let before = case.counts().await;
        let failure = tasks
            .bind(case.run(), &case.plan("two-objectives", 2))
            .await
            .expect_err("two objectives cannot fit the configured one-task cap");
        assert_eq!(failure.code(), error_code::TASK_LIMIT_EXHAUSTED);
        assert_eq!(
            case.counts().await,
            before,
            "the first insertion rolled back too"
        );
        let accepted = tasks
            .bind(case.run(), &case.plan("one-objective", 1))
            .await
            .expect("one objective fits");
        assert_eq!(accepted.items.len(), 1);
        assert_eq!(case.counts().await, (1, 1, 1));
    })
    .await;
}
