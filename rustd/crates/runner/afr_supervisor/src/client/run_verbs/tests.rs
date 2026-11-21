//! The schedules and messages verbs on the wire: method, path, fence.

#![expect(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test target: a fixture that cannot be built is a broken test, and a step \
              indexes the calls it just sent"
)]

use af_core::id::Uuid7;
use afr_tools::ScheduleCall;

use crate::client::{Method, Verb};
use crate::test_support::{FENCING, RUN_ID, drain, json, plane};

/// A schedule id, as `cron_list` names one.
const SCHEDULE: &str = "01890a5d-ac96-774b-bcce-b302099a80aa";

/// What one call put on the wire: verb, method, path, and body as JSON.
type Sent = (Verb, Method, String, Option<serde_json::Value>);

/// Sends every schedules call and the message, and answers what went out.
async fn sent() -> Vec<Sent> {
    let (plane, mut calls) = plane(|_call| json(&serde_json::json!({"delivered": true})));
    let run = Uuid7::parse(RUN_ID).unwrap();
    let schedule = Uuid7::parse(SCHEDULE).unwrap();
    let schedule_calls = [
        ScheduleCall::Create {
            call_number: 1,
            cron: "0 9 * * 1",
            timezone: Some("Asia/Kolkata"),
            message: "weekly check",
            once: true,
        },
        ScheduleCall::List,
        ScheduleCall::Update {
            call_number: 2,
            schedule: &schedule,
            cron: None,
            timezone: None,
            message: Some("daily check"),
            paused: Some(false),
        },
        ScheduleCall::Delete {
            schedule: &schedule,
        },
        ScheduleCall::Run {
            call_number: 3,
            schedule: &schedule,
        },
        ScheduleCall::Runs {
            schedule: &schedule,
            limit: Some(2),
            starting_after: Some("a b&c=d"),
        },
    ];
    for call in schedule_calls {
        plane.schedules(&run, FENCING, call).await.unwrap();
    }
    plane.message(&run, FENCING, 4, "fix pushed").await.unwrap();
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
async fn every_run_verb_carries_its_method_path_and_fence() {
    let collection = format!("/v1/runners/me/runs/{RUN_ID}/schedules");
    let member = format!("{collection}/{SCHEDULE}");
    let fence = format!("fencing_token={FENCING}");
    let calls = sent().await;
    let routes: Vec<(Verb, Method, &str)> = calls
        .iter()
        .map(|(verb, method, path, _body)| (*verb, *method, path.as_str()))
        .collect();
    assert_eq!(
        routes,
        vec![
            (Verb::ScheduleCreate, Method::Post, collection.as_str()),
            (
                Verb::ScheduleList,
                Method::Get,
                format!("{collection}?{fence}").as_str()
            ),
            (Verb::ScheduleUpdate, Method::Patch, member.as_str()),
            (
                Verb::ScheduleDelete,
                Method::Delete,
                format!("{member}?{fence}").as_str()
            ),
            (
                Verb::ScheduleRun,
                Method::Post,
                format!("{member}/runs").as_str()
            ),
            (
                Verb::ScheduleRuns,
                Method::Get,
                format!("{member}/runs?{fence}&limit=2&starting_after=a+b%26c%3Dd").as_str()
            ),
            (
                Verb::Message,
                Method::Post,
                format!("/v1/runners/me/runs/{RUN_ID}/messages").as_str()
            ),
        ]
    );
}

/// Every body carries the run's fence and the fields its call named, and no
/// read or delete has a body at all.
#[tokio::test]
async fn every_run_verb_body_carries_its_fence_and_fields() {
    let calls = sent().await;
    for (verb, method, _path, body) in &calls {
        match method {
            Method::Get | Method::Delete => assert!(body.is_none(), "{verb:?}"),
            Method::Post | Method::Patch | Method::Put => {
                let token = body
                    .as_ref()
                    .and_then(|body| body["fencing_token"].as_u64());
                assert_eq!(token, Some(FENCING), "{verb:?}");
            }
        }
    }
    let create = calls[0].3.as_ref().unwrap();
    assert_eq!(create["once"], true);
    assert_eq!(create["timezone"], "Asia/Kolkata");
    let update = calls[2].3.as_ref().unwrap();
    assert_eq!(update["paused"], false);
    assert_eq!(update["message"], "daily check");
    assert_eq!(calls[6].3.as_ref().unwrap()["text"], "fix pushed");
}

/// A create, an edit, a run-now and a message each name their call's item:
/// a minted id and the call's number in its run.
#[tokio::test]
async fn every_effect_body_names_its_item() {
    let calls = sent().await;
    for (index, call_number) in [(0, 1), (2, 2), (4, 3), (6, 4)] {
        let body = calls[index].3.as_ref().unwrap();
        assert_eq!(body["call_number"], call_number, "{:?}", calls[index].0);
        let item_id = body["item_id"].as_str().unwrap();
        assert!(Uuid7::parse(item_id).is_ok(), "{item_id}");
    }
}

/// A message that met a blip is sent again under the same item, so the
/// daemon answers the retry with the first post's result.
#[tokio::test(start_paused = true)]
async fn a_retried_message_names_the_same_item() {
    let attempts = std::sync::atomic::AtomicUsize::new(0);
    let (plane, mut calls) = plane(move |call| {
        if attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
            crate::test_support::Answer::Fail(crate::error::unavailable(call.verb, 503))
        } else {
            json(&serde_json::json!({"delivered": true}))
        }
    });
    let run = Uuid7::parse(RUN_ID).unwrap();

    plane.message(&run, FENCING, 7, "fix pushed").await.unwrap();

    let sent = drain(&mut calls);
    let items: Vec<serde_json::Value> = sent
        .iter()
        .map(|call| {
            let body: serde_json::Value =
                serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
            body["item_id"].clone()
        })
        .collect();
    assert_eq!(items.len(), 2, "one blip, one retry");
    assert_eq!(items[0], items[1]);
}

/// Each method reaches the HTTP client as the method it names; a PATCH sent
/// as a POST would be refused by the route, and a DELETE as a GET would read.
#[test]
fn every_method_is_sent_as_itself() {
    let cases = [
        (Method::Get, reqwest::Method::GET),
        (Method::Post, reqwest::Method::POST),
        (Method::Put, reqwest::Method::PUT),
        (Method::Patch, reqwest::Method::PATCH),
        (Method::Delete, reqwest::Method::DELETE),
    ];
    for (method, http) in cases {
        assert_eq!(method.http(), http, "{method:?}");
    }
}

/// The fleets reads carry the fence and the page in their query, and the
/// steer posts its fenced body to the run's steers path.
#[tokio::test]
async fn the_fleet_verbs_reach_their_paths_with_the_fence() {
    let (plane, mut calls) = plane(|_call| json(&serde_json::json!({})));
    let run = Uuid7::parse(RUN_ID).unwrap();
    let fleet = Uuid7::parse(SCHEDULE).unwrap();
    let list = afr_tools::FleetsCall::List {
        limit: Some(3),
        starting_after: None,
    };
    let events = afr_tools::FleetsCall::Events {
        fleet: &fleet,
        limit: None,
        starting_after: Some("c 1"),
    };
    plane.fleets(&run, FENCING, list).await.unwrap();
    plane.fleets(&run, FENCING, events).await.unwrap();
    plane
        .fleet_steer(&run, FENCING, &fleet, "run the check")
        .await
        .unwrap();
    let sent = drain(&mut calls);
    let root = format!("/v1/runners/me/runs/{RUN_ID}");
    let fence = format!("fencing_token={FENCING}");
    assert_eq!(sent[0].verb, Verb::FleetList);
    assert_eq!(sent[0].path, format!("{root}/fleets?{fence}&limit=3"));
    assert_eq!(sent[1].verb, Verb::FleetTurns);
    assert_eq!(
        sent[1].path,
        format!("{root}/fleets/{SCHEDULE}/turns?{fence}&starting_after=c+1")
    );
    assert_eq!(sent[2].verb, Verb::FleetSteer);
    assert_eq!(sent[2].path, format!("{root}/steers"));
    let body: serde_json::Value = serde_json::from_slice(sent[2].body.as_ref().unwrap()).unwrap();
    assert_eq!(body["fencing_token"].as_u64(), Some(FENCING));
    assert_eq!(body["fleet_id"], SCHEDULE);
    assert_eq!(body["text"], "run the check");
}
