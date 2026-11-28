//! The fenced supervisor passes one search or extract outcome back to the tool.
use af_core::{error_code, id::Uuid7};
use afr_tools::{RunVerbs as _, Unanswered, WebUnanswered};
use serde_json::{Value, json};

use super::{FencedVerbs, Host};
use crate::client::Verb;
use crate::error;
use crate::test_support::{Answer, FENCING, RUN_ID, drain, plane};

const URL: &str = "https://example.org/guide";

#[tokio::test]
async fn a_fenced_web_call_preserves_the_problem_and_explicit_local_permission() {
    let (plane, mut calls) = plane(|call| {
        let body: Value = serde_json::from_slice(call.body.as_ref().unwrap()).unwrap();
        assert_eq!(
            body.get("fencing_token").and_then(Value::as_u64),
            Some(FENCING)
        );
        match call.verb {
            Verb::WebSearch => {
                assert_eq!(body.get("query").and_then(Value::as_str), Some("migration"));
                assert_eq!(body.get("call_number").and_then(Value::as_u64), Some(1));
                let problem = json!({"error_code":error_code::WEB_RESERVATION_UNAVAILABLE});
                Answer::Fail(error::unavailable_with_body(
                    call.verb,
                    503,
                    &serde_json::to_vec(&problem).unwrap(),
                ))
            }
            Verb::WebExtract => {
                assert_eq!(body.get("url").and_then(Value::as_str), Some(URL));
                match body.get("call_number").and_then(Value::as_u64).unwrap() {
                    2 => {
                        let problem = json!({"error_code":error_code::WEB_EXTRACT_UNAVAILABLE,"local_read_permitted":true});
                        Answer::Fail(error::unavailable_with_body(
                            call.verb,
                            502,
                            &serde_json::to_vec(&problem).unwrap(),
                        ))
                    }
                    3 => Answer::Fail(error::refused(
                        call.verb,
                        403,
                        Some(error_code::WEB_NETWORK_RESTRICTED),
                    )),
                    4 => Answer::Reply(bytes::Bytes::new()),
                    _ => unreachable!("unexpected extract"),
                }
            }
            _ => unreachable!("unexpected verb"),
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
    assert_eq!(verbs.web_timeout(), plane.tuning.call_timeout);
    assert_eq!(
        verbs.web_search(1, "migration").await,
        Err(Unanswered::Refused(Some(
            error_code::WEB_RESERVATION_UNAVAILABLE
        )))
    );
    for (number, failure) in [
        (2, WebUnanswered::LocalReadPermitted),
        (
            3,
            WebUnanswered::Unanswered(Unanswered::Refused(Some(
                error_code::WEB_NETWORK_RESTRICTED,
            ))),
        ),
        (4, WebUnanswered::Unanswered(Unanswered::Unreachable)),
    ] {
        assert_eq!(verbs.web_extract(number, URL).await, Err(failure));
    }
    assert_eq!(drain(&mut calls).len(), 4);
}
