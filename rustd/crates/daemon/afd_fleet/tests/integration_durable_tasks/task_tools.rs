//! A run's `task_start` binds one span of a person's message to a ready task
//! and its continuation, and nothing else may.

use af_core::{error_code, etag::sha256_hex, id::Uuid7};
use af_wire::decision::NoulCriteria;
use af_wire::durable_task::{
    DurableTaskBindingItem, DurableTaskBindingsInput, DurableTaskRequestedTarget, DurableTaskState,
    DurableTaskStatement,
};
use afd_fleet::durable_task::DurableTaskLifecycle as _;
use afd_turns::Producer;

use super::fixture::{Case, GOALS, exercise};

/// One completion statement a task is started with.
pub(super) fn statement(id: &str) -> DurableTaskStatement {
    DurableTaskStatement {
        id: id.to_owned(),
        question: "Does the repaired code pass its regression?".to_owned(),
        criteria: NoulCriteria {
            yes: "The regression passes at the current head.".to_owned(),
            no: "It fails, or current evidence is missing.".to_owned(),
        },
    }
}

/// The digest the daemon computes for `input`, so the binding is the one sent.
pub(super) fn sealed(mut input: DurableTaskBindingsInput) -> DurableTaskBindingsInput {
    let digest = sha256_hex(&serde_json::to_vec(&input.interpretation()).expect("a binding"));
    input.plan_digest = digest;
    input
}

/// The span of the case's message that names `GOALS[index]`.
pub(super) fn goal_span(index: usize) -> (i32, i32) {
    let message = GOALS.join("; ");
    let goal = GOALS.get(index).expect("a fixture goal");
    let start = message.find(goal).expect("the goal is in the message");
    (
        i32::try_from(start).expect("a small offset"),
        i32::try_from(start + goal.len()).expect("a small offset"),
    )
}

/// The `task_start` a run sends for `span` of `source`, titled `title`.
pub(super) fn task_start(
    case: &Case,
    source: &Uuid7,
    (start, end): (i32, i32),
    title: &str,
    statements: Vec<DurableTaskStatement>,
) -> DurableTaskBindingsInput {
    sealed(DurableTaskBindingsInput {
        fencing_token: case.run().fencing_token,
        source_turn_id: source.to_string(),
        plan_digest: "0".repeat(64),
        items: vec![DurableTaskBindingItem::NewWork {
            source_start: start,
            source_end: end,
            title: title.to_owned(),
            target: DurableTaskRequestedTarget::ApiOnly,
            statements,
        }],
    })
}

/// `input` as the run verb's extractor proves it.
pub(super) fn valid(input: DurableTaskBindingsInput) -> garde::Valid<DurableTaskBindingsInput> {
    garde::Unvalidated::new(input)
        .validate()
        .expect("a binding inside its bounds")
}

/// The task `case`'s head message starts for its first goal.
pub(super) async fn started(case: &Case) -> Uuid7 {
    let input = task_start(
        case,
        &case.held.turn_id,
        goal_span(0),
        GOALS[0],
        vec![statement("fixed")],
    );
    let bound = case
        .tasks
        .interpret(case.run(), &valid(input))
        .await
        .expect("a person's message starts a task");
    let task = bound
        .items
        .first()
        .and_then(|item| item.durable_task_id.as_deref());
    Uuid7::parse(task.expect("the item owns a task")).expect("a task id")
}

/// The unsettled continuation turns `task` holds.
pub(super) async fn continuations(case: &Case, task: &Uuid7) -> i64 {
    sqlx::query_scalar(
        "SELECT count(*) FROM thread.turns WHERE durable_task_id = $1 AND settled_at IS NULL",
    )
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
    .expect("the task's continuations")
}

/// The decision calls the case's fleet made.
pub(super) async fn decision_calls(case: &Case) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM runner.decision_calls WHERE fleet_id = $1")
        .bind(&case.held.fleet)
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
        .expect("the fleet's decision calls")
}

// Value: protects=one task_start writes a ready task whose questions are the
// call's, stages its continuation and leaves the calling run unbound, with no
// decision call; fails_when=the binding binds the run, skips the continuation,
// or rebuilds the questions; seam=`DurableTasks::interpret` over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_task_start_writes_a_ready_task_and_its_continuation() {
    exercise(async |case| {
        let task = started(case).await;

        let read = case
            .tasks
            .read(&case.scope, &task)
            .await
            .expect("the task reads");
        assert_eq!(read.context.state, DurableTaskState::Ready);
        let names: Vec<&str> = read
            .context
            .questions
            .iter()
            .map(|q| q.name.as_str())
            .collect();
        assert_eq!(names, ["goal.fixed"]);
        assert_eq!(continuations(case, &task).await, 1);
        let bound: Option<Uuid7> =
            sqlx::query_scalar("SELECT durable_task_id FROM thread.runs WHERE id = $1")
                .bind(&case.held.run_key)
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
                .expect("the calling run");
        assert_eq!(bound, None, "the calling run stays an ordinary run");
        assert_eq!(decision_calls(case).await, 0);
    })
    .await;
}

// Value: protects=only a person's message starts a task; fails_when=a
// webhook's, a schedule's or a machine's turn is taken as a person's words;
// seam=`DurableTasks::interpret` over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_task_start_under_a_webhook_turn_is_refused() {
    exercise(async |case| {
        let person = afd_turns::view::steer_actor("native_durable_task_owner");
        for (producer, actor) in [
            (Producer::Webhook, person.as_str()),
            (Producer::ScheduleFire, person.as_str()),
            (Producer::Steer, "service:automation"),
        ] {
            let source = case.source(producer, actor).await;
            case.assign(&source).await;
            let before = case.counts().await;
            let input = task_start(
                case,
                &source,
                goal_span(0),
                GOALS[0],
                vec![statement("fixed")],
            );

            let refused = case.tasks.interpret(case.run(), &valid(input)).await;

            assert_eq!(
                refused.err().map(|refusal| refusal.code()),
                Some(error_code::DURABLE_TASK_INVALID_TARGET),
                "{producer:?}"
            );
            assert_eq!(case.counts().await.0, before.0, "no task row");
        }
    })
    .await;
}

// Value: protects=a task's objective is a span of the person's message the run
// holds; fails_when=a span past the message, or a message the run never held,
// starts a task; seam=`DurableTasks::interpret` over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_task_start_with_a_span_outside_the_message_is_refused() {
    exercise(async |case| {
        let past = task_start(
            case,
            &case.held.turn_id,
            (0, 10_000),
            "Past",
            vec![statement("fixed")],
        );
        let person = afd_turns::view::steer_actor("native_durable_task_owner");
        let unheld = case.source(Producer::Steer, &person).await;
        let foreign = task_start(
            case,
            &unheld,
            goal_span(0),
            GOALS[0],
            vec![statement("fixed")],
        );

        let beyond = case.tasks.interpret(case.run(), &valid(past)).await;
        let unheld = case.tasks.interpret(case.run(), &valid(foreign)).await;

        assert_eq!(
            beyond.err().map(|refusal| refusal.code()),
            Some(error_code::DURABLE_TASK_REPLAY_CONFLICT)
        );
        assert_eq!(
            unheld.err().map(|refusal| refusal.code()),
            Some(error_code::DURABLE_TASK_STALE_STATE)
        );
        assert_eq!(case.counts().await.0, 0, "no task row");
    })
    .await;
}

// Value: protects=a task needs a statement to be judged by; fails_when=an
// empty statement list passes the bound and starts an unjudgeable task;
// seam=the binding's garde bound, and the store it leaves untouched.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_task_start_with_no_statement_is_refused() {
    exercise(async |case| {
        let input = task_start(case, &case.held.turn_id, goal_span(0), GOALS[0], Vec::new());

        garde::Unvalidated::new(input)
            .validate()
            .expect_err("an empty statement list is out of bounds");
        assert_eq!(case.counts().await.0, 0, "no task row");
    })
    .await;
}

// Value: protects=one source turn takes one plan: the same call answers the
// first task and another is refused; fails_when=a retried task_start makes a
// second task, or another plan under the turn is accepted; seam=
// `DurableTasks::interpret` over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn a_second_plan_under_one_turn_is_refused() {
    exercise(async |case| {
        let first = started(case).await;
        let other = task_start(
            case,
            &case.held.turn_id,
            goal_span(1),
            GOALS[1],
            vec![statement("other")],
        );
        let same = task_start(
            case,
            &case.held.turn_id,
            goal_span(0),
            GOALS[0],
            vec![statement("fixed")],
        );

        let refused = case.tasks.interpret(case.run(), &valid(other)).await;
        let replay = case
            .tasks
            .interpret(case.run(), &valid(same))
            .await
            .expect("the same call is answered");

        assert_eq!(
            refused.err().map(|refusal| refusal.code()),
            Some(error_code::DURABLE_TASK_REPLAY_CONFLICT)
        );
        assert!(replay.replayed);
        let task = replay
            .items
            .first()
            .and_then(|item| item.durable_task_id.as_deref());
        assert_eq!(task, Some(first.as_str()));
        assert_eq!(case.counts().await.0, 1, "one task");
    })
    .await;
}
