//! Paid association keeps existing-task input on its original execution.
use af_core::{error_code, etag::sha256_hex, id::Uuid7};
use af_wire::{
    decision::{DecisionAnswer, DecisionOutcome},
    durable_task::{
        DecisionAssessment, DecisionReceipt, DurableTaskBindingItem, DurableTaskBindingsInput,
        DurableTaskInputAction, DurableTaskProposedItem,
    },
};
use afd_fleet::durable_task::DurableTaskLifecycle as _;
use serde_json::json;

use super::{
    super::fixture::{Case, exercise, id},
    fixture::{Decisions, request},
    interpretation::choose,
    reply::known_result,
};

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn paid_steer_and_refresh_append_once_to_the_running_durable_task() {
    for action in [
        DurableTaskInputAction::Steer,
        DurableTaskInputAction::Refresh,
    ] {
        exercise(async |case| {
            let fixture = Decisions::create(case).await;
            let task = fixture.bind_goal().await;
            let durable_task_id = id(&task.durable_task_id);
            let source = case.assigned_source().await;
            let input = proposed(&fixture, &source, &task.durable_task_id, action).await;
            let accepted = fixture
                .tasks
                .interpret(case.run(), &valid(input.clone()))
                .await
                .expect("paid existing association");
            assert!(!accepted.replayed);
            let item = accepted.items.first().expect("one associated item");
            assert_eq!(accepted.items.len(), 1);
            assert_eq!(
                item.durable_task_id.as_deref(),
                Some(task.durable_task_id.as_str())
            );
            assert_eq!(item.action, action);
            assert!(item.applies_here);
            assert_eq!(item.input_sequence, Some(1));
            assert_eq!(item.durable_task_generation, Some(1));
            assert_input(case, &durable_task_id, action).await;
            let replay = fixture
                .tasks
                .interpret(case.run(), &valid(input))
                .await
                .expect("accepted association replay");
            assert!(replay.replayed);
            assert_eq!(replay.items, accepted.items);
            assert_input(case, &durable_task_id, action).await;
            assert_eq!(fixture.counts().await, (1, 1, 1));
            fixture.model.finish().await.expect("model fixture cleanup");
        })
        .await;
    }
}

#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn paid_existing_input_refuses_missing_and_foreign_durable_task_without_partial_writes() {
    exercise(async |case| {
        let fixture = Decisions::create(case).await;
        let task = fixture.bind_goal().await;
        let durable_task_id = id(&task.durable_task_id);
        let source = case.assigned_source().await;
        let input = proposed(
            &fixture,
            &source,
            &task.durable_task_id,
            DurableTaskInputAction::Steer,
        )
        .await;
        let before = case.counts().await;
        for (target, code) in [
            (None, error_code::DURABLE_TASK_INVALID_TARGET),
            (
                Some(afd_db::test_util::mint_id()),
                error_code::DURABLE_TASK_NOT_FOUND,
            ),
        ] {
            let mut wrong = input.clone();
            wrong
                .items
                .first_mut()
                .expect("one item")
                .proposed
                .durable_task_id = target.map(String::from);
            digest(&mut wrong);
            let error = fixture
                .tasks
                .interpret(case.run(), &valid(wrong))
                .await
                .expect_err("unowned association refused");
            assert_eq!(error.code(), code);
            assert_eq!(case.counts().await, before);
            let count: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM fleet.durable_task_inputs WHERE durable_task_id=$1",
            )
            .bind(&durable_task_id)
            .fetch_one(
                &mut *case
                    .held
                    .fixtures
                    .database
                    .acquire()
                    .await
                    .expect("input observer"),
            )
            .await
            .expect("unchanged inputs");
            assert_eq!(count, 0);
        }
        assert_eq!(fixture.counts().await, (1, 1, 1));
        fixture.model.finish().await.expect("model fixture cleanup");
    })
    .await;
}

// Value: protects=a person's steer wakes a blocked task into exactly one
// continuation at the next generation; fails_when=the steer appends without
// queueing, or queues at the stale generation; why_new=the wake path of an
// existing-task input had no case; seam=the paid interpretation over live
// Postgres, the task stopped as its settlement would leave it.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn paid_steer_wakes_a_blocked_durable_task_into_one_continuation() {
    exercise(async |case| {
        let fixture = Decisions::create(case).await;
        let task = fixture.bind_goal().await;
        let durable_task_id = id(&task.durable_task_id);
        stop(case, &durable_task_id, "blocked", false).await;
        let source = case.assigned_source().await;
        let input = proposed_from(
            &fixture,
            &source,
            &task.durable_task_id,
            DurableTaskInputAction::Steer,
            false,
        )
        .await;
        let accepted = fixture
            .tasks
            .interpret(case.run(), &valid(input))
            .await
            .expect("a steer wakes the blocked task");
        let item = accepted.items.first().expect("one associated item");
        assert!(!item.applies_here, "the woken task runs on its own turn");
        assert_eq!(item.input_sequence, Some(1));
        assert_eq!(item.durable_task_generation, Some(3));
        let (state, queued): (String, i64) = sqlx::query_as(
            "SELECT t.status,(SELECT count(*) FROM thread.turns a WHERE a.durable_task_id=t.id AND a.durable_task_generation=t.generation AND a.settled_at IS NULL) FROM fleet.durable_tasks t WHERE t.id=$1",
        )
        .bind(&durable_task_id)
        .fetch_one(
            &mut *case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("woken task observer"),
        )
        .await
        .expect("woken task");
        assert_eq!((state.as_str(), queued), ("ready", 1));
        fixture.model.finish().await.expect("model fixture cleanup");
    })
    .await;
}

// Value: protects=a steer cannot wake a task with no run left, nor revive a
// finished one; fails_when=either input queues another execution;
// why_new=the exhausted and terminal refusals of an existing-task input had no
// case; seam=the paid interpretation over live Postgres.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn paid_steer_refuses_to_wake_an_exhausted_or_finished_durable_task() {
    for (state, exhausted, code) in [
        ("blocked", true, error_code::DURABLE_TASK_LIMIT_EXHAUSTED),
        ("done", false, error_code::DURABLE_TASK_STALE_STATE),
    ] {
        exercise(async |case| {
            let fixture = Decisions::create(case).await;
            let task = fixture.bind_goal().await;
            let durable_task_id = id(&task.durable_task_id);
            stop(case, &durable_task_id, state, exhausted).await;
            let source = case.assigned_source().await;
            let input = proposed_from(
                &fixture,
                &source,
                &task.durable_task_id,
                DurableTaskInputAction::Steer,
                false,
            )
            .await;
            let before = case.counts().await;
            let error = fixture
                .tasks
                .interpret(case.run(), &valid(input))
                .await
                .expect_err("no further execution is admitted");
            assert_eq!(error.code(), code, "{state}");
            assert_eq!(case.counts().await, before, "{state}");
            fixture.model.finish().await.expect("model fixture cleanup");
        })
        .await;
    }
}

/// Stops `task` as a settlement would: the next generation, in `state`, and
/// the run that executed it now serves the conversation alone.
async fn stop(case: &Case, task: &Uuid7, state: &str, exhausted: bool) {
    let mut connection = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("stop connection");
    sqlx::query("UPDATE thread.runs SET durable_task_id=NULL,durable_task_generation=NULL,durable_task_stop_generation=NULL WHERE id=$1")
        .bind(&case.held.run_key)
        .execute(&mut *connection)
        .await
        .expect("run released from the stopped task");
    let stopped = sqlx::query(
        "UPDATE fleet.durable_tasks SET status=$2,generation=generation+1,stop_generation=stop_generation+1,used_runs=CASE WHEN $3 THEN max_runs ELSE used_runs END WHERE id=$1",
    )
    .bind(task)
    .bind(state)
    .bind(exhausted)
    .execute(&mut *connection)
    .await
    .expect("stopped task");
    assert_eq!(stopped.rows_affected(), 1);
}

pub(super) async fn proposed(
    fixture: &Decisions<'_>,
    source: &Uuid7,
    task: &str,
    action: DurableTaskInputAction,
) -> DurableTaskBindingsInput {
    proposed_from(fixture, source, task, action, true).await
}

/// [`proposed`], from a run `attached` to the task or from one that is not.
async fn proposed_from(
    fixture: &Decisions<'_>,
    source: &Uuid7,
    task: &str,
    action: DurableTaskInputAction,
    attached: bool,
) -> DurableTaskBindingsInput {
    let reservation = fixture.reservation(
        DecisionAssessment::Message,
        attached.then(|| af_wire::durable_task::DecisionDurableTaskVersion {
            durable_task_id: task.to_owned(),
            generation: 1,
        }),
    );
    let mut reservation = (*reservation).clone();
    reservation.source_turn_id = source.to_string();
    let reservation = garde::Unvalidated::new(reservation)
        .validate()
        .expect("assigned message reservation");
    let (_, body) = fixture.reserve(&reservation).await;
    let request = request(&body);
    let mut result = known_result(&request);
    let answers = match &mut result.result {
        DecisionOutcome::Answered { answers } => Some(answers),
        _ => None,
    }
    .expect("complete decision answers");
    answers.insert(
        "multiple_requests".to_owned(),
        DecisionAnswer::Noul { noul: 0.0 },
    );
    choose(answers, "intent", action.as_str());
    choose(answers, "association", "t0");
    fixture
        .record(&request.call_id, DecisionReceipt::Provider { result })
        .await;
    binding(fixture, source, task, action, request.call_id)
}

fn binding(
    fixture: &Decisions<'_>,
    source: &Uuid7,
    task: &str,
    action: DurableTaskInputAction,
    call: String,
) -> DurableTaskBindingsInput {
    let mut input = DurableTaskBindingsInput {
        fencing_token: fixture.case.run().fencing_token,
        source_turn_id: source.to_string(),
        accepted_revision_id: fixture.revision.to_string(),
        plan_digest: "0".repeat(64),
        message_decision_call_id: call,
        items: vec![DurableTaskBindingItem {
            decision_call_id: None,
            proposed: DurableTaskProposedItem {
                source_start: 0,
                source_end: 6,
                action,
                durable_task_id: Some(task.to_owned()),
                target: None,
                title: "Repair title decoding".to_owned(),
                interpretation: "Preserve title decoding behavior".to_owned(),
            },
        }],
    };
    digest(&mut input);
    input
}

fn digest(input: &mut DurableTaskBindingsInput) {
    let digest =
        sha256_hex(&serde_json::to_vec(&input.interpretation()).expect("complete interpretation"));
    input.plan_digest = digest;
}

fn valid(input: DurableTaskBindingsInput) -> garde::Valid<DurableTaskBindingsInput> {
    garde::Unvalidated::new(input)
        .validate()
        .expect("bounded interpretation")
}

async fn assert_input(case: &Case, task: &Uuid7, action: DurableTaskInputAction) {
    let mut connection = case
        .held
        .fixtures
        .database
        .acquire()
        .await
        .expect("accepted input observer");
    let rows: Vec<(i64, String, String)> = sqlx::query_as("SELECT sequence,accepted_text,interpretation FROM fleet.durable_task_inputs WHERE durable_task_id=$1")
        .bind(task).fetch_all(&mut *connection).await.expect("accepted input custody");
    assert_eq!(
        rows,
        [(
            1,
            "Repair".to_owned(),
            "Preserve title decoding behavior".to_owned(),
        )]
    );
    let state: (String, i64, i64, i64) = sqlx::query_as("SELECT status,generation,input_sequence,(SELECT count(*) FROM thread.turns WHERE thread_id=$2) FROM fleet.durable_tasks WHERE id=$1")
        .bind(task).bind(&case.scope.fleet_id).fetch_one(&mut *connection).await.expect("same execution");
    assert_eq!(state, ("running".to_owned(), 1, 1, 2));
    let saved: serde_json::Value = sqlx::query_scalar(
        "SELECT to_jsonb(i) FROM fleet.durable_task_items i WHERE durable_task_id=$1 AND action=$2",
    )
    .bind(task)
    .bind(action.as_str())
    .fetch_one(&mut *connection)
    .await
    .expect("original accepted action");
    assert_eq!(saved.get("action"), Some(&json!(action.as_str())));
}
