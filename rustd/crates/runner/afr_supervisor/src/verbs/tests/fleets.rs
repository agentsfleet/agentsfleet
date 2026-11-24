//! The fleets and steers verbs as a tool reads them: the daemon's own text,
//! the event a steer became, and each refusal keeping its code.

#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use af_core::error_code;
use af_core::id::Uuid7;
use afr_tools::{RunVerbs as _, Unanswered};

use super::{FencedVerbs, Host};
use crate::client::Verb;
use crate::error;
use crate::test_support::{Answer, FENCING, RUN_ID, json, plane};

/// A steer answers the event it became, owned past the reply it came in.
#[tokio::test]
async fn a_fleet_steer_answers_the_event_it_became() {
    let (plane, _calls) = plane(|call| match call.verb {
        Verb::FleetSteer => json(&serde_json::json!({"turn_id": "e-1", "replayed": true})),
        _other => json(&serde_json::json!({})),
    });
    let run = Uuid7::parse(RUN_ID).unwrap();
    let fleet = Uuid7::parse(RUN_ID).unwrap();
    let host = Host::new();
    let verbs = FencedVerbs::new(
        &plane,
        &run,
        FENCING,
        &host.halt,
        &host.stop,
        &host.decisions,
    );
    let accepted = verbs.fleet_steer(&fleet, "hello").await.unwrap();
    assert_eq!(accepted.turn_id, "e-1");
    assert!(accepted.replayed);
}

/// A fleets page reaches the tool as the daemon's own text.
#[tokio::test]
async fn a_fleets_reply_reaches_the_tool_as_its_text() {
    let (plane, mut calls) = plane(|call| {
        assert_eq!(call.verb, Verb::FleetList);
        Answer::Reply(bytes::Bytes::from_static(br#"{"fleets":[]}"#))
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
    let listed = verbs
        .fleets(afr_tools::FleetsCall::List {
            limit: Some(5),
            starting_after: None,
        })
        .await;
    assert_eq!(listed.as_deref(), Ok(r#"{"fleets":[]}"#));
    assert_eq!(crate::test_support::drain(&mut calls).len(), 1);
}

/// A refused fleets read keeps its code; a daemon out of reach is unreachable.
#[tokio::test]
async fn a_failed_fleets_read_keeps_its_refusal_or_is_unreachable() {
    for (status, expected) in [
        (
            409,
            Unanswered::Refused(Some(error_code::AGENTSFLEET_NOT_FOUND)),
        ),
        (503, Unanswered::Unreachable),
    ] {
        let (plane, _calls) = plane(move |call| {
            Answer::Fail(if status == 503 {
                error::unavailable(call.verb, status)
            } else {
                error::refused(call.verb, status, Some(error_code::AGENTSFLEET_NOT_FOUND))
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
        let listed = verbs
            .fleets(afr_tools::FleetsCall::List {
                limit: None,
                starting_after: None,
            })
            .await;
        assert_eq!(listed, Err(expected), "{status}");
    }
}

/// A steer the daemon refused keeps its code, and one whose reply is not
/// the documented shape is no answer.
#[tokio::test]
async fn a_refused_or_unreadable_fleet_steer_is_no_accepted_event() {
    let fleet = Uuid7::parse(RUN_ID).unwrap();
    for refused in [true, false] {
        let (plane, _calls) = plane(move |call| {
            if refused {
                Answer::Fail(error::refused(
                    call.verb,
                    409,
                    Some(error_code::AGENTSFLEET_NOT_FOUND),
                ))
            } else {
                json(&serde_json::json!({"turn": "e-1"}))
            }
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
        let steered = verbs.fleet_steer(&fleet, "hello").await;
        let expected = if refused {
            Unanswered::Refused(Some(error_code::AGENTSFLEET_NOT_FOUND))
        } else {
            Unanswered::Unreachable
        };
        assert_eq!(steered.map(|accepted| accepted.replayed), Err(expected));
    }
}
