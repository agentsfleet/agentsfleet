//! What a steer request's declared bounds refuse, and what its published
//! description promises about the fields it does not read.
//!
//! The at-limit and one-past pairs follow the method `validation.rs` explains.
//! A steer adds one rule no length expresses: neither its message nor its
//! operation id may hold NUL, because Postgres cannot store one, and a value
//! accepted here that the database then refuses is a 202 that never runs.

use std::borrow::Cow;

use afd_wire::event::{
    OPERATION_ID_MAX_BYTES, STEER_MESSAGE_MAX_BYTES, SteerAccepted, SteerRequest,
};
use garde::Validate as _;

use crate::validation::of_len;

/// A steer message is bounded, and its optional operation id is bounded only
/// when present.
///
/// `garde(inner(...))` is the arm that makes `None` legal while a present value
/// is still held to its bounds — the distinction a hand-written check gets
/// wrong by rejecting absence.
#[test]
fn a_steer_request_bounds_its_message_and_its_optional_operation_id() {
    let at_limit = SteerRequest {
        message: Cow::Owned(of_len(STEER_MESSAGE_MAX_BYTES)),
        operation_id: Some(Cow::Owned(of_len(OPERATION_ID_MAX_BYTES))),
    };
    assert!(at_limit.validate().is_ok(), "a steer at its limits");

    let absent_id = SteerRequest {
        message: Cow::Borrowed("restart the run"),
        operation_id: None,
    };
    assert!(absent_id.validate().is_ok(), "an absent operation id");

    let long_message = SteerRequest {
        message: Cow::Owned(of_len(STEER_MESSAGE_MAX_BYTES + 1)),
        operation_id: None,
    };
    assert!(long_message.validate().is_err(), "a message past its cap");

    let empty_message = SteerRequest {
        message: Cow::Borrowed(""),
        operation_id: None,
    };
    assert!(empty_message.validate().is_err(), "an empty message");

    let long_id = SteerRequest {
        message: Cow::Borrowed("restart the run"),
        operation_id: Some(Cow::Owned(of_len(OPERATION_ID_MAX_BYTES + 1))),
    };
    assert!(long_id.validate().is_err(), "an operation id past its cap");

    let empty_id = SteerRequest {
        message: Cow::Borrowed("restart the run"),
        operation_id: Some(Cow::Borrowed("")),
    };
    assert!(empty_id.validate().is_err(), "a present but empty id");

    let nul_id = SteerRequest {
        message: Cow::Borrowed("restart the run"),
        operation_id: Some(Cow::Borrowed("op\u{0}1")),
    };
    assert!(nul_id.validate().is_err(), "an id holding NUL");
}

/// The request's documentation and its parser agree: unknown fields are
/// refused, and the published description says so.
#[test]
fn test_steer_request_doc_matches_its_parser() {
    let unknown =
        serde_json::from_str::<SteerRequest<'_>>(r#"{"message":"restart","priority":"high"}"#);
    assert!(unknown.is_err(), "an unknown field is refused");

    let openapi = include_str!("../../../../public/openapi.json");
    let description = serde_json::from_str::<serde_json::Value>(openapi)
        .ok()
        .and_then(|document| {
            document
                .pointer("/components/schemas/SteerRequest/description")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        });
    assert!(
        description
            .as_deref()
            .is_some_and(|text| text.contains("Unknown fields are refused")),
        "the published SteerRequest says what its parser does: {description:?}"
    );
}

/// Each field's published description states the bound its rule enforces, so a
/// constant that moves without its sentence fails here rather than in a client.
#[test]
fn a_steer_field_description_names_its_bound_and_the_nul_rule() {
    let openapi = include_str!("../../../../public/openapi.json");
    let document = serde_json::from_str::<serde_json::Value>(openapi).unwrap_or_default();
    for (field, bound) in [
        ("message", STEER_MESSAGE_MAX_BYTES),
        ("operation_id", OPERATION_ID_MAX_BYTES),
    ] {
        let pointer = format!("/components/schemas/SteerRequest/properties/{field}/description");
        let text = document
            .pointer(&pointer)
            .and_then(serde_json::Value::as_str)
            .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        assert!(
            text.contains(&format!("1 to {bound} bytes")),
            "{field}: {text}"
        );
        assert!(text.contains("no NUL character"), "{field}: {text}");
    }
}

/// A message holding NUL is refused wherever it sits, and one without is not.
///
/// The length rule alone accepted it, so the lease's `jsonb` cast was the first
/// thing to object, after the caller had been answered 202.
#[test]
fn test_steer_message_holding_nul_is_refused() {
    for message in ["\u{0}", "ship\u{0}it", "ship it\u{0}"] {
        let holding = SteerRequest {
            message: Cow::Borrowed(message),
            operation_id: None,
        };
        assert!(holding.validate().is_err(), "{message:?} holds NUL");
    }
    let clean = SteerRequest {
        message: Cow::Borrowed("ship it \u{2728}"),
        operation_id: None,
    };
    assert!(clean.validate().is_ok(), "a message without NUL");
}

/// The steer's answer tolerates a field this build does not know, unlike the
/// request: a reply a newer daemon extends must still parse in an older client.
#[test]
#[expect(clippy::expect_used, reason = "the test inspects a JSON document")]
fn test_steer_answer_tolerates_new_fields() {
    let answer =
        r#"{"status":"accepted","event_id":"1790573387481-566","replayed":false,"added_later":1}"#;
    let parsed: SteerAccepted<'_> =
        serde_json::from_str(answer).expect("an extended answer parses");
    assert_eq!(parsed.event_id, "1790573387481-566");
    assert!(!parsed.replayed);

    let request = r#"{"message":"ship it","added_later":1}"#;
    assert!(
        serde_json::from_str::<SteerRequest<'_>>(request).is_err(),
        "the request still refuses a field it does not read",
    );
}
