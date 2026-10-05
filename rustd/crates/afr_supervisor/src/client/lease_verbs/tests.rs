//! The schedules and messages verbs on the wire: method, path, fence.

#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use afd_core::id::Uuid7;
use afr_agent::ScheduleCall;

use crate::client::{Method, Verb};
use crate::test_support::{FENCING, LEASE_ID, drain, json, plane};

/// A schedule id, as `cron_list` names one.
const SCHEDULE: &str = "01890a5d-ac96-774b-bcce-b302099a80aa";

/// What one call put on the wire: verb, method, path, and body as JSON.
type Sent = (Verb, Method, String, Option<serde_json::Value>);

/// Sends every schedules call and the message, and answers what went out.
async fn sent() -> Vec<Sent> {
    let (plane, mut calls) = plane(|_call| json(&serde_json::json!({"delivered": true})));
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let schedule = Uuid7::parse(SCHEDULE).unwrap();
    let schedule_calls = [
        ScheduleCall::Create {
            cron: "0 9 * * 1",
            timezone: Some("Asia/Kolkata"),
            message: "weekly check",
            once: true,
        },
        ScheduleCall::List,
        ScheduleCall::Update {
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
            schedule: &schedule,
        },
        ScheduleCall::Runs {
            schedule: &schedule,
            limit: Some(2),
            starting_after: Some("a b&c=d"),
        },
    ];
    for call in schedule_calls {
        plane.schedules(&lease, FENCING, call).await.unwrap();
    }
    plane.message(&lease, FENCING, "fix pushed").await.unwrap();
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
async fn every_lease_verb_carries_its_method_path_and_fence() {
    let collection = format!("/v1/runners/me/leases/{LEASE_ID}/schedules");
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
                format!("/v1/runners/me/leases/{LEASE_ID}/messages").as_str()
            ),
        ]
    );
    // Every body carries the lease's fence, and no read or delete has one.
    for (verb, method, _path, body) in &calls {
        match method {
            Method::Get | Method::Delete => assert!(body.is_none(), "{verb:?}"),
            Method::Post | Method::Patch => {
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
