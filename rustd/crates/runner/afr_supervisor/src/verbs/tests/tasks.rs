//! Task tools retain the supervisor's fence and never retry a lost write.

use super::*;
use af_wire::task::{
    DecisionTaskVersion, TaskBindingItem, TaskBindingsInput, TaskEffectInput, TaskEffectsQuery,
    TaskFileChange, TaskInputAction, TaskInputMode, TaskInputReceiptInput, TaskInputsQuery,
    TaskProposedItem,
};
use garde::Validate as _;

const ID: &str = "019f0000-0000-7000-8000-000000000001";

#[tokio::test]
async fn input_reads_use_the_supervisor_fence_without_retrying_failures() {
    one_attempt(Verb::TaskInputs, async |verbs| {
        let query = TaskInputsQuery {
            fencing_token: 0,
            starting_after: Some(
                af_core::paging::Cursor::Text {
                    value: "7".into(),
                    id: ID.into(),
                }
                .to_string(),
            ),
            after_input_sequence: Some(7),
            limit: 4,
        };
        query.validate().unwrap();
        verbs.task_inputs(query).await.map(drop)
    })
    .await;
}

#[tokio::test]
async fn bindings_use_the_supervisor_fence_without_retrying_failures() {
    one_attempt(Verb::TaskBindings, async |verbs| {
        let mut bindings = TaskBindingsInput {
            fencing_token: 0,
            source_turn_id: ID.to_owned(),
            accepted_revision_id: ID.to_owned(),
            plan_digest: "a".repeat(64),
            message_decision_call_id: ID.to_owned(),
            items: vec![TaskBindingItem {
                proposed: TaskProposedItem {
                    source_start: 0,
                    source_end: 6,
                    action: TaskInputAction::Status,
                    task_id: Some(ID.to_owned()),
                    title: "Status".to_owned(),
                    interpretation: "Report progress".to_owned(),
                    target: None,
                },
                decision_call_id: None,
            }],
        };
        let digest =
            af_core::etag::sha256_hex(&serde_json::to_vec(&bindings.interpretation()).unwrap());
        bindings.plan_digest = digest;
        bindings.validate().unwrap();
        verbs.task_bindings(bindings).await.map(drop)
    })
    .await;
}

#[tokio::test]
async fn input_receipts_use_the_supervisor_fence_without_retrying_failures() {
    one_attempt(Verb::TaskInputReceipt, async |verbs| {
        let receipt = TaskInputReceiptInput {
            fencing_token: 0,
            operation_id: ID.to_owned(),
            source_turn_id: ID.to_owned(),
            plan_digest: "a".repeat(64),
            item_id: ID.to_owned(),
            task: Some(DecisionTaskVersion {
                task_id: ID.to_owned(),
                generation: 2,
            }),
            mode: TaskInputMode::Answered,
            response_text: Some("Review complete".to_owned()),
        };
        receipt.validate().unwrap();
        verbs.task_input_receipt(receipt).await.map(drop)
    })
    .await;
}

#[tokio::test]
async fn publication_use_the_supervisor_fence_without_retrying_failures() {
    one_attempt(Verb::TaskEffectPublish, async |verbs| {
        let publication = TaskEffectInput {
            operation_id: ID.to_owned(),
            fencing_token: 0,
            task: DecisionTaskVersion {
                task_id: ID.to_owned(),
                generation: 2,
            },
            source_item_id: ID.to_owned(),
            covered_input_sequence: 7,
            expected_head: "a".repeat(40),
            tested_tree: "b".repeat(40),
            changes: vec![TaskFileChange {
                path: "review.txt".to_owned(),
                contents: Some("b2s=".to_owned()),
            }],
        };
        publication.validate().unwrap();
        verbs.task_publish(publication).await.map(drop)
    })
    .await;
}

#[tokio::test]
async fn effect_pages_use_the_supervisor_fence_without_retrying_failures() {
    one_attempt(Verb::TaskEffects, async |verbs| {
        let effects = TaskEffectsQuery {
            fencing_token: 0,
            task_id: ID.to_owned(),
            ordinal_after: 3,
            starting_after: Some("3".to_owned()),
            limit: 1,
        };
        effects.validate().unwrap();
        verbs.task_effects(effects).await.map(drop)
    })
    .await;
}

#[tokio::test]
async fn effect_read_uses_the_supervisor_fence_without_retrying_failures() {
    one_attempt(Verb::TaskEffect, async |verbs| {
        verbs.task_effect(ID).await.map(drop)
    })
    .await;
}

async fn one_attempt(
    verb: Verb,
    invoke: impl for<'a> AsyncFn(&FencedVerbs<'a>) -> Result<(), Unanswered>,
) {
    for unavailable in [false, true] {
        let (plane, mut calls) = plane(move |call| {
            Answer::Fail(if unavailable {
                error::unavailable(call.verb, 503)
            } else {
                error::refused(call.verb, 409, Some(error_code::TASK_STALE_STATE))
            })
        });
        let run = Uuid7::parse(RUN_ID).unwrap();
        let host = Host::new();
        let verbs = FencedVerbs::new(
            &plane,
            &run,
            FENCING,
            &host.halt,
            &host.stop,
            &host.decisions,
        );
        let expected = if unavailable {
            Unanswered::Unreachable
        } else {
            Unanswered::Refused(Some(error_code::TASK_STALE_STATE))
        };
        assert_eq!(invoke(&verbs).await, Err(expected));
        let requests = crate::test_support::drain(&mut calls);
        assert_eq!(requests.len(), 1);
        let call = requests.first().unwrap();
        assert_eq!(call.verb, verb);
        assert!(call.path.contains(RUN_ID), "{call:?}");
        assert_fence(call);
    }
}

fn assert_fence(call: &crate::client::Call) {
    if let Some(body) = &call.body {
        let body: serde_json::Value = serde_json::from_slice(body).unwrap();
        assert_eq!(
            body.get("fencing_token")
                .and_then(serde_json::Value::as_u64),
            Some(FENCING)
        );
    } else {
        let url = url::Url::parse(&format!("https://daemon.invalid{}", call.path)).unwrap();
        let query: std::collections::BTreeMap<_, _> = url.query_pairs().collect();
        assert_eq!(
            query.get("fencing_token").map(ToString::to_string),
            Some(FENCING.to_string())
        );
    }
}

#[tokio::test]
async fn an_effect_identifier_that_is_not_a_uuid_is_refused_without_a_call() {
    let (plane, mut calls) = plane(|_call| Answer::Fail(error::config("unexpected effect read")));
    let run = Uuid7::parse(RUN_ID).unwrap();
    let host = Host::new();
    let verbs = FencedVerbs::new(
        &plane,
        &run,
        FENCING,
        &host.halt,
        &host.stop,
        &host.decisions,
    );
    assert_eq!(
        verbs.task_effect("not-an-effect").await.err(),
        Some(Unanswered::Refused(Some(
            error_code::INTERNAL_OPERATION_FAILED
        )))
    );
    assert_eq!(crate::test_support::drain(&mut calls).len(), 0);
}
