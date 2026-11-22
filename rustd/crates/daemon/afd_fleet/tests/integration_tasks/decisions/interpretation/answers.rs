//! Only a current provider answer that clears the reviewed routing thresholds
//! authorizes an interpretation; every other answer refuses it whole.

use std::collections::BTreeMap;

use af_core::error_code::{self, ErrorCode};
use af_wire::{
    decision::{DecisionAnswer, DecisionOutcome, DecisionResult},
    task::{
        DecisionAssessment, DecisionNotSentCause, DecisionReceipt, TaskBindingItem,
        TaskBindingsInput, TaskInputAction, TaskProposedItem, TaskRequestedTarget,
    },
};
use afd_fleet::task::TaskLifecycle as _;

use super::{Case, Decisions, choose, digest, exercise, known_result, request, valid};

/// The two words of the held turn's message the items here interpret.
const REPAIR: (i32, i32) = (0, 6);
const TITLE: (i32, i32) = (7, 12);

/// Reserves `assessment` and records its provider answers as `edit` leaves
/// them, returning the call's id.
async fn answered(
    fixture: &Decisions<'_>,
    assessment: DecisionAssessment,
    edit: impl FnOnce(&mut DecisionResult),
) -> String {
    let (issued, body) = fixture
        .reserve(&fixture.reservation(assessment, None))
        .await;
    assert!(issued, "a fresh reservation is sent");
    let request = request(&body);
    let mut result = known_result(&request);
    edit(&mut result);
    fixture
        .record(&request.call_id, DecisionReceipt::Provider { result })
        .await;
    request.call_id
}

fn answers(result: &mut DecisionResult) -> &mut BTreeMap<String, DecisionAnswer> {
    match &mut result.result {
        DecisionOutcome::Answered { answers } => Some(answers),
        _ => None,
    }
    .expect("the fixture answers every question")
}

/// A message decision that reads one request asking for `action`.
async fn single(fixture: &Decisions<'_>, action: &str) -> String {
    answered(fixture, DecisionAssessment::Message, |result| {
        let answers = answers(result);
        answers.insert(
            "multiple_requests".to_owned(),
            DecisionAnswer::Noul { noul: 0.0 },
        );
        choose(answers, "intent", action);
    })
    .await
}

fn item(span: (i32, i32), action: TaskInputAction) -> TaskBindingItem {
    TaskBindingItem {
        decision_call_id: None,
        proposed: TaskProposedItem {
            source_start: span.0,
            source_end: span.1,
            action,
            task_id: None,
            target: None,
            title: "Discuss the request".to_owned(),
            interpretation: "Talk the request through before accepting any work".to_owned(),
        },
    }
}

fn plan(fixture: &Decisions<'_>, call: &str, items: Vec<TaskBindingItem>) -> TaskBindingsInput {
    let mut input = TaskBindingsInput {
        fencing_token: fixture.case.run().fencing_token,
        source_turn_id: fixture.case.held.turn_id.clone().into(),
        accepted_revision_id: fixture.revision.to_string(),
        plan_digest: "0".repeat(64),
        message_decision_call_id: call.to_owned(),
        items,
    };
    digest(&mut input);
    input
}

/// `input` is refused with `code`, and the turn holds no interpretation and no
/// task afterwards.
async fn refused(fixture: &Decisions<'_>, input: TaskBindingsInput, code: ErrorCode) {
    let case = fixture.case;
    let failure = fixture
        .tasks
        .interpret(case.run(), &valid(input))
        .await
        .expect_err("the interpretation is not authorized");
    assert_eq!(failure.code(), code, "{failure}");
    assert_eq!(interpretations(case).await, 0, "nothing was interpreted");
    assert_eq!(case.counts().await.0, 0, "no task was created");
}

async fn interpretations(case: &Case) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM fleet.task_interpretations WHERE turn_id=$1")
        .bind(&case.held.turn_id)
        .fetch_one(
            &mut *case
                .held
                .fixtures
                .database
                .acquire()
                .await
                .expect("interpretation observer"),
        )
        .await
        .expect("interpretations are countable")
}

// Value: protects=an interpretation is authorized only by a decision that was
// sent, answered by a reviewed model, and is still current; fails_when=the load
// stops refusing an unrecorded call, a not-sent receipt, an unreviewed model's
// answers, or a call whose run changed profile; why_new=these refusals of the
// decision load had no case; seam=the production interpret verb over live
// Postgres, each call recorded through the production receipt verb;
// no_claim=threshold routing, which the cases below cover.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_interpretation_needs_a_sent_reviewed_and_current_decision() {
    exercise(async |case| {
        let fixture = Decisions::create(case).await;
        let (_, body) = fixture
            .reserve(&fixture.reservation(DecisionAssessment::Message, None))
            .await;
        let unrecorded = request(&body).call_id;
        let conversation = || vec![item(REPAIR, TaskInputAction::Conversation)];
        refused(
            &fixture,
            plan(&fixture, &unrecorded, conversation()),
            error_code::TASK_STALE_STATE,
        )
        .await;
        fixture
            .record(
                &unrecorded,
                DecisionReceipt::NotSent {
                    cause: DecisionNotSentCause::Preflight,
                },
            )
            .await;
        refused(
            &fixture,
            plan(&fixture, &unrecorded, conversation()),
            error_code::TASK_DECISION_UNAVAILABLE,
        )
        .await;
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
    exercise(async |case| {
        let fixture = Decisions::create(case).await;
        let call = answered(&fixture, DecisionAssessment::Message, |result| {
            result.reported_model = Some("an-unreviewed-model".to_owned());
        })
        .await;
        refused(
            &fixture,
            plan(
                &fixture,
                &call,
                vec![item(REPAIR, TaskInputAction::Conversation)],
            ),
            error_code::TASK_STALE_STATE,
        )
        .await;
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
    exercise(async |case| {
        let fixture = Decisions::create(case).await;
        let call = single(&fixture, "conversation").await;
        sqlx::query("UPDATE thread.runs SET decision_snapshot = NULL WHERE id=$1")
            .bind(&case.held.run_key)
            .execute(
                &mut *case
                    .held
                    .fixtures
                    .database
                    .acquire()
                    .await
                    .expect("profile writer"),
            )
            .await
            .expect("the run's decision profile is withdrawn");
        refused(
            &fixture,
            plan(
                &fixture,
                &call,
                vec![item(REPAIR, TaskInputAction::Conversation)],
            ),
            error_code::TASK_STALE_STATE,
        )
        .await;
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
}

// Value: protects=a request count or intent the model was unsure of never routes
// work; fails_when=the request-count band, the intent confidence floor or the
// missing-confidence refusal is dropped; why_new=these threshold refusals had no
// case; seam=the production interpret verb over live Postgres; no_claim=the
// item-level calls of a multiple-request message, covered below.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_unsure_request_count_or_intent_is_ambiguous() {
    type Edit = fn(&mut BTreeMap<String, DecisionAnswer>);
    let edits: [Edit; 3] = [
        |answers| {
            answers.insert(
                "multiple_requests".to_owned(),
                DecisionAnswer::Noul { noul: 0.5 },
            );
        },
        |answers| {
            answers.insert(
                "multiple_requests".to_owned(),
                DecisionAnswer::Noul { noul: 0.0 },
            );
            choose(answers, "intent", "conversation");
            if let Some(DecisionAnswer::Choice { confidence, .. }) = answers.get_mut("intent") {
                *confidence = None;
            }
        },
        |answers| {
            answers.insert(
                "multiple_requests".to_owned(),
                DecisionAnswer::Noul { noul: 0.0 },
            );
            choose(answers, "intent", "conversation");
            if let Some(DecisionAnswer::Choice { confidence, .. }) = answers.get_mut("intent") {
                *confidence = Some(0.5);
            }
        },
    ];
    for edit in edits {
        exercise(async |case| {
            let fixture = Decisions::create(case).await;
            let call = answered(&fixture, DecisionAssessment::Message, |result| {
                edit(answers(result));
            })
            .await;
            refused(
                &fixture,
                plan(
                    &fixture,
                    &call,
                    vec![item(REPAIR, TaskInputAction::Conversation)],
                ),
                error_code::TASK_INPUT_AMBIGUOUS,
            )
            .await;
            fixture.model.finish().await.expect("model cleanup");
        })
        .await;
    }
}

// Value: protects=an item must match the one request the model read: its count,
// its target and its association; fails_when=a single-request answer accepts two
// items, a conversation carries a target, or new work is accepted without a
// new-work association; why_new=these
// routing refusals had no case; seam=the production interpret verb over live
// Postgres, all against one recorded decision; no_claim=existing-task routing.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn an_item_that_disagrees_with_the_read_request_is_refused() {
    exercise(async |case| {
        let fixture = Decisions::create(case).await;
        let conversation = single(&fixture, "conversation").await;
        let two = vec![
            item(REPAIR, TaskInputAction::Conversation),
            item(TITLE, TaskInputAction::Conversation),
        ];
        refused(
            &fixture,
            plan(&fixture, &conversation, two),
            error_code::TASK_INPUT_AMBIGUOUS,
        )
        .await;
        let mut targeted = item(REPAIR, TaskInputAction::Conversation);
        targeted.proposed.target = Some(TaskRequestedTarget::ApiOnly);
        refused(
            &fixture,
            plan(&fixture, &conversation, vec![targeted]),
            error_code::TASK_INVALID_TARGET,
        )
        .await;
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
    exercise(async |case| {
        let fixture = Decisions::create(case).await;
        // The intent reads new work, and the association finds it unclear.
        let call = answered(&fixture, DecisionAssessment::Message, |result| {
            let answers = answers(result);
            answers.insert(
                "multiple_requests".to_owned(),
                DecisionAnswer::Noul { noul: 0.0 },
            );
            choose(answers, "intent", "new_work");
            choose(answers, "association", "unclear");
        })
        .await;
        let mut work = item(REPAIR, TaskInputAction::NewWork);
        work.proposed.target = Some(TaskRequestedTarget::ApiOnly);
        refused(
            &fixture,
            plan(&fixture, &call, vec![work]),
            error_code::TASK_INPUT_AMBIGUOUS,
        )
        .await;
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
}

// Value: protects=a message the model read as several requests is accepted only
// with one item decision per item, in order; fails_when=the item calls are not
// loaded, their ordinal or plan is not compared, or one item decision authorizes
// two items; why_new=the multiple-request branch had no live case; seam=the
// production reserve, receipt and interpret verbs over live Postgres;
// no_claim=new-work items, which need a target the conversation items here omit.
#[tokio::test]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn several_requests_are_accepted_only_with_one_ordered_item_decision_each() {
    exercise(async |case| {
        let fixture = Decisions::create(case).await;
        // The fixture's provider answers every noul with one: several requests.
        let message = answered(&fixture, DecisionAssessment::Message, |_| {}).await;
        let mut input = plan(
            &fixture,
            &message,
            vec![
                item(REPAIR, TaskInputAction::Conversation),
                item(TITLE, TaskInputAction::Conversation),
            ],
        );
        refused(
            &fixture,
            plan(
                &fixture,
                &message,
                vec![item(REPAIR, TaskInputAction::Conversation)],
            ),
            error_code::TASK_INPUT_AMBIGUOUS,
        )
        .await;
        let mut calls = Vec::new();
        for (ordinal, span) in [(0, REPAIR), (1, TITLE)] {
            let assessment = DecisionAssessment::Item {
                ordinal,
                interpretation_digest: input.plan_digest.clone(),
                source_start: span.0,
                source_end: span.1,
            };
            calls.push(
                answered(&fixture, assessment, |result| {
                    choose(answers(result), "intent", "conversation");
                })
                .await,
            );
        }
        let first = calls.first().expect("the first item's call").clone();
        let mut reused = input.clone();
        for item in &mut reused.items {
            item.decision_call_id = Some(first.clone());
        }
        refused(&fixture, reused, error_code::TASK_REPLAY_CONFLICT).await;
        for (item, call) in input.items.iter_mut().zip(&calls) {
            item.decision_call_id = Some(call.clone());
        }
        let accepted = fixture
            .tasks
            .interpret(case.run(), &valid(input))
            .await
            .expect("each item carries its own ordered decision");
        assert!(!accepted.replayed);
        assert_eq!(
            accepted
                .items
                .iter()
                .map(|item| (item.action, item.task_id.is_none()))
                .collect::<Vec<_>>(),
            vec![
                (TaskInputAction::Conversation, true),
                (TaskInputAction::Conversation, true),
            ]
        );
        assert_eq!(interpretations(case).await, 1);
        fixture.model.finish().await.expect("model cleanup");
    })
    .await;
}
