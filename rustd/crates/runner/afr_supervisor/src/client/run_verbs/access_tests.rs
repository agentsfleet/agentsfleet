//! The triggers and access verbs on the wire: method, path, fence and body.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: a fixture that cannot be built is a broken test, and a step \
              indexes the calls it just sent"
)]

use std::borrow::Cow;

use af_core::id::Uuid7;
use af_wire::access_verb::AccessRequest;
use af_wire::trigger_verb::{TriggerKind, TriggerVerifier};
use afr_tools::verbs::{NewTrigger, TriggerCall};

use crate::client::{Method, Verb};
use crate::error;
use crate::test_support::{Answer, FENCING, RUN_ID, drain, json, plane};

/// A trigger id, as `trigger_list` names one.
const TRIGGER: &str = "01890a5d-ac96-774b-bcce-b302099a80bb";

/// What one call put on the wire: verb, method, path, and body as JSON.
type Sent = (Verb, Method, String, Option<serde_json::Value>);

/// A webhook from Sentry, signed with the vault's `sentry-hook`.
fn sentry() -> NewTrigger<'static> {
    NewTrigger {
        kind: TriggerKind::Webhook,
        source: Some(Cow::Borrowed("sentry")),
        events: Some(vec![Cow::Borrowed("issue.created")]),
        repositories: None,
        verifier: Some(TriggerVerifier {
            scheme: Cow::Borrowed("body-hex-sha256"),
            header: None,
        }),
        secret_name: Some(Cow::Borrowed("sentry-hook")),
        channel: None,
    }
}

/// Sends every triggers call and one access request, and answers what went
/// out.
async fn sent() -> Vec<Sent> {
    let (plane, mut calls) = plane(|_call| json(&serde_json::json!({})));
    let run = Uuid7::parse(RUN_ID).unwrap();
    let trigger = Uuid7::parse(TRIGGER).unwrap();
    let trigger_calls = [
        TriggerCall::Add {
            call_number: 3,
            trigger: sentry(),
        },
        TriggerCall::List,
        TriggerCall::Remove { trigger: &trigger },
    ];
    for call in trigger_calls {
        plane.triggers(&run, FENCING, call).await.unwrap();
    }
    let request = AccessRequest {
        fencing_token: 0,
        name: Cow::Borrowed("github"),
        scope: Some(serde_json::Map::from_iter([(
            "access".to_owned(),
            serde_json::json!("read"),
        )])),
    };
    plane.access(&run, FENCING, request).await.unwrap();
    drain(&mut calls)
        .into_iter()
        .map(|call| {
            let body = call
                .body
                .as_ref()
                .map(|bytes| serde_json::from_slice(bytes).unwrap());
            (call.verb, call.verb.method(), call.path.into_owned(), body)
        })
        .collect()
}

#[tokio::test]
async fn every_trigger_and_access_verb_carries_its_method_and_path() {
    let collection = format!("/v1/runners/me/runs/{RUN_ID}/triggers");
    let fence = format!("fencing_token={FENCING}");
    let calls = sent().await;
    let routes: Vec<(Verb, Method, &str)> = calls
        .iter()
        .map(|(verb, method, path, _body)| (*verb, *method, path.as_str()))
        .collect();
    assert_eq!(
        routes,
        vec![
            (Verb::TriggerAdd, Method::Post, collection.as_str()),
            (
                Verb::TriggerList,
                Method::Get,
                format!("{collection}?{fence}").as_str()
            ),
            (
                Verb::TriggerRemove,
                Method::Delete,
                format!("{collection}/{TRIGGER}?{fence}").as_str()
            ),
            (
                Verb::Access,
                Method::Post,
                format!("/v1/runners/me/runs/{RUN_ID}/access").as_str()
            ),
        ]
    );
}

/// An add carries the run's fence, a minted item and the call's number with
/// the trigger as named; an access request carries the run's fence whatever
/// the tool sent, and its name and scope; a read or a removal has no body.
#[tokio::test]
async fn every_trigger_and_access_body_carries_the_runs_fence_and_fields() {
    let calls = sent().await;
    let add = calls[0].3.as_ref().unwrap();
    assert_eq!(add["fencing_token"], FENCING);
    assert_eq!(add["call_number"], 3);
    let item = add["item_id"].as_str().unwrap();
    assert_eq!(Uuid7::parse(item).unwrap().as_str(), item);
    assert_eq!(add["kind"], "webhook");
    assert_eq!(add["source"], "sentry");
    assert_eq!(add["events"], serde_json::json!(["issue.created"]));
    assert_eq!(add["verifier"]["scheme"], "body-hex-sha256");
    assert_eq!(add["secret_name"], "sentry-hook");
    assert_eq!((&calls[1].3, &calls[2].3), (&None, &None));
    assert_eq!(
        calls[3].3,
        Some(
            serde_json::json!({"fencing_token": FENCING, "name": "github",
            "scope": {"access": "read"}})
        )
    );
}

/// An add names its item, so a blip is retried with the same one and the
/// daemon answers the first trigger; an access request is sent once.
#[tokio::test(start_paused = true)]
async fn an_add_is_retried_under_its_item_and_an_access_request_is_not() {
    let (plane, mut calls) = plane(|call| Answer::Fail(error::unavailable(call.verb, 503)));
    let run = Uuid7::parse(RUN_ID).unwrap();
    let add = TriggerCall::Add {
        call_number: 1,
        trigger: sentry(),
    };
    let failed = plane.triggers(&run, FENCING, add).await.unwrap_err();
    assert!(failed.is_retryable(), "{failed}");
    let adds = drain(&mut calls);
    assert!(adds.len() > 1, "a blip is retried: {}", adds.len());
    let items: Vec<serde_json::Value> = adds
        .iter()
        .map(|call| serde_json::from_slice::<serde_json::Value>(call.body.as_ref().unwrap()))
        .map(|body| body.unwrap()["item_id"].clone())
        .collect();
    assert!(items.iter().all(|item| *item == items[0]), "{items:?}");

    let request = AccessRequest {
        fencing_token: 0,
        name: Cow::Borrowed("sentry"),
        scope: None,
    };
    let failed = plane.access(&run, FENCING, request).await.unwrap_err();
    assert!(failed.is_retryable(), "{failed}");
    assert_eq!(drain(&mut calls).len(), 1);
}
