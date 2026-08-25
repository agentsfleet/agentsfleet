//! Dimension 5.4 — error responses match the shape the Zig daemon emits.
//!
//! The envelope is a contract with clients that already exist: the dashboard
//! reads `user_message`, the CLI reads `error_code`, and support reads
//! `request_id` off a screenshot. Every assertion here is against
//! `src/agentsfleetd/http/handlers/problem_response.zig` and its test file, so
//! a change that looks harmless from inside Rust fails here rather than in
//! somebody's browser.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use afd_api::{CONTENT_TYPE_PROBLEM_JSON, ProblemResponse};
use afd_core::error_code;

/// A detail long enough that a truncating envelope would be caught.
///
/// The number is arbitrary; that it is the SAME number in the input and the
/// assertion is not, which is the whole reason it is bound to a name.
const LONG_DETAIL_BYTES: usize = 1000;
use afd_core::problem::Problem;
use axum::body::to_bytes;
use axum::response::IntoResponse;
use http::header;
use serde_json::Value;

/// Renders a refusal the way axum will, and reads the body back as JSON.
async fn render(problem: ProblemResponse) -> (u16, String, Value) {
    let response = problem.into_response();
    let status = response.status().as_u16();
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("a problem body is small and in memory");
    let json = serde_json::from_slice(&bytes).expect("the envelope must be valid JSON");
    (status, content_type, json)
}

/// The base envelope carries exactly the five fields every refusal has.
#[tokio::test]
async fn test_the_base_envelope_is_the_five_fields_and_no_others() {
    let (status, content_type, json) = render(ProblemResponse::new(
        error_code::AUTH_UNAUTHORIZED,
        "test detail",
        "req-001",
    ))
    .await;

    assert_eq!(status, 401, "the code owns its status, not the call site");
    assert_eq!(content_type, CONTENT_TYPE_PROBLEM_JSON);

    let object = json.as_object().expect("the envelope is an object");
    let mut keys: Vec<&str> = object.keys().map(String::as_str).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec!["detail", "docs_uri", "error_code", "request_id", "title"],
        "the base envelope gained or lost a field"
    );

    assert_eq!(object["detail"], "test detail");
    assert_eq!(object["error_code"], "UZ-AUTH-002");
    assert_eq!(object["request_id"], "req-001");
}

/// The documentation link comes from the registry, never from the caller.
///
/// A caller-built link is how a code ends up pointing at another code's anchor
/// — the reader lands on a page describing an error they did not get.
#[tokio::test]
async fn test_the_docs_link_is_the_registry_entry_for_that_code() {
    let (_status, _ct, json) = render(ProblemResponse::new(
        error_code::AUTH_UNAUTHORIZED,
        "d",
        "r",
    ))
    .await;
    let expected = Problem::of(error_code::AUTH_UNAUTHORIZED);

    assert_eq!(json["docs_uri"], expected.docs_uri());
    assert_eq!(json["title"], expected.title());
    assert!(
        json["docs_uri"]
            .as_str()
            .unwrap_or_default()
            .ends_with("UZ-AUTH-002"),
        "the link must anchor on the code it describes"
    );
}

/// `hint` is operator-facing and never reaches a client.
///
/// It tells whoever runs this what to go and look at. On the wire it would be
/// internal remediation advice handed to anyone who can provoke an error, and
/// the Zig envelope has never carried it.
#[tokio::test]
async fn test_hint_never_reaches_the_client() {
    for problem in afd_core::problem::entries() {
        let (_status, _ct, json) = render(ProblemResponse::new(problem.code(), "d", "r")).await;
        assert!(
            json.get("hint").is_none(),
            "{} put its operator hint on the wire",
            problem.code().as_str()
        );
        let rendered = json.to_string();
        assert!(
            !rendered.contains(problem.hint()),
            "{}'s hint text leaked into the envelope",
            problem.code().as_str()
        );
    }
}

/// An absent extension is absent, not null.
///
/// RFC 7807 defines extensions per status, so a client is invited to switch on
/// key presence. Emitting `"etag": null` on every response makes that switch
/// wrong everywhere.
#[tokio::test]
async fn test_absent_extensions_are_omitted_rather_than_nulled() {
    let (_status, _ct, json) = render(ProblemResponse::new(
        error_code::AUTH_UNAUTHORIZED,
        "d",
        "r",
    ))
    .await;
    let object = json.as_object().expect("an object");

    for absent in ["current_state", "etag"] {
        assert!(
            !object.contains_key(absent),
            "{absent} is present on a response whose status does not define it"
        );
    }
}

/// A conflict names the state that forbade the transition; a precondition
/// failure names the version the client raced with.
///
/// Each rides the base envelope rather than replacing it, and each appears
/// only on its own response.
#[tokio::test]
async fn test_each_extension_rides_its_own_status_only() {
    let (_status, _ct, conflict) = render(ProblemResponse::conflict(
        error_code::AUTH_UNAUTHORIZED,
        "cannot start a paused fleet",
        "req-409",
        "paused",
    ))
    .await;
    assert_eq!(conflict["current_state"], "paused");
    assert!(
        conflict.get("etag").is_none(),
        "a conflict must not carry a precondition's field"
    );
    assert_eq!(
        conflict["detail"], "cannot start a paused fleet",
        "the extension rides the base envelope, it does not replace it"
    );

    let (_status, _ct, precondition) = render(ProblemResponse::precondition_failed(
        error_code::AUTH_UNAUTHORIZED,
        "the fleet changed under you",
        "req-412",
        "W/\"v7\"",
    ))
    .await;
    assert_eq!(precondition["etag"], "W/\"v7\"");
    assert!(
        precondition.get("current_state").is_none(),
        "a precondition failure must not carry a conflict's field"
    );
}

/// `user_message` is present verbatim where a code has one, and absent where
/// it does not — never an empty string.
///
/// The dashboard renders it to a person. An empty string would render as a
/// blank space where a sentence should be, which reads as a broken page rather
/// than as an error with no curated wording.
#[tokio::test]
async fn test_user_message_is_verbatim_or_absent() {
    let mut with_message = 0_usize;
    for problem in afd_core::problem::entries() {
        let (_status, _ct, json) = render(ProblemResponse::new(problem.code(), "d", "r")).await;
        match problem.user_message() {
            Some(message) => {
                assert_eq!(
                    json["user_message"],
                    message,
                    "{} must carry its curated sentence verbatim",
                    problem.code().as_str()
                );
                with_message += 1;
            }
            None => assert!(
                json.get("user_message").is_none(),
                "{} has no curated sentence, so the field must be absent",
                problem.code().as_str()
            ),
        }
    }
    assert!(
        with_message > 0,
        "no code carries a user_message — the registry or this test is wrong"
    );
}

/// A long detail travels whole.
///
/// The Zig writer truncates nothing, and a detail cut mid-sentence is how the
/// one line explaining a failure loses the part that explained it.
#[tokio::test]
async fn test_a_long_detail_is_not_truncated() {
    let detail = "d".repeat(LONG_DETAIL_BYTES);
    let (_status, _ct, json) = render(ProblemResponse::new(
        error_code::AUTH_UNAUTHORIZED,
        detail.clone(),
        "req-t2c",
    ))
    .await;

    assert_eq!(
        json["detail"].as_str().unwrap_or_default().len(),
        LONG_DETAIL_BYTES,
        "the detail was truncated somewhere"
    );
    assert_eq!(json["detail"], detail);
}

/// Every registered code answers the status its registry entry names.
#[tokio::test]
async fn test_every_code_answers_its_registered_status() {
    for problem in afd_core::problem::entries() {
        let (status, content_type, json) =
            render(ProblemResponse::new(problem.code(), "d", "r")).await;
        assert_eq!(
            status,
            problem.status(),
            "{} answered a status its registry entry does not name",
            problem.code().as_str()
        );
        assert_eq!(content_type, CONTENT_TYPE_PROBLEM_JSON);
        assert_eq!(json["error_code"], problem.code().as_str());
    }
}
