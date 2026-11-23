//! What a setup message may be, and the sentence a refused one is answered
//! with, decided before the setup domain is called.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use af_validate::{TEXT_MAX_CHARS, TEXT_TOO_LONG};
use axum::response::IntoResponse as _;
use http::StatusCode;
use serde_json::{Value, json};

use super::{INVALID, create, send};
use crate::handler::Refusal;

/// A retry key the body carries, a canonical `UUIDv7`.
const OPERATION: &str = "0190d5c2-7e1a-7000-8000-00000000e001";

/// A two-byte character, so every bound below is proved in characters.
const WIDE: char = 'é';

/// A draft message body carrying `message`.
fn message_body(message: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({"message": message, "operation_id": OPERATION}))
        .expect("a message body serializes")
}

/// A blank draft's creation body carrying `message` as its first message.
fn create_body(message: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "source": {"kind": "blank"},
        "setup_model_ref": "anthropic/claude-sonnet",
        "message": message,
        "operation_id": OPERATION,
    }))
    .expect("a creation body serializes")
}

/// The status and `detail` a refusal answers with.
async fn answer(refusal: Refusal) -> (StatusCode, String) {
    let (status, body) = problem(refusal).await;
    let detail = body
        .get("detail")
        .and_then(Value::as_str)
        .expect("a detail")
        .to_owned();
    (status, detail)
}

/// The status and problem body a refusal answers with.
async fn problem(refusal: Refusal) -> (StatusCode, Value) {
    let response = refusal.into_response();
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("the refusal body");
    (
        status,
        serde_json::from_slice(&bytes).expect("a problem body"),
    )
}

/// 1,048,577 characters is one past the ceiling: the send and a draft's
/// first message are refused with the sentence that names 1,048,576, before
/// the setup domain is called, so nothing is stored. The refusal carries its
/// own code, whose dashboard sentence is the one the setup chat shows.
#[tokio::test]
async fn a_message_past_the_ceiling_is_refused_naming_it() {
    let past: String = std::iter::repeat_n(WIDE, TEXT_MAX_CHARS + 1).collect();
    for refusal in [
        send(&message_body(&past)).expect_err("a send past the ceiling is refused"),
        create(&create_body(&past)).expect_err("a first message past the ceiling is refused"),
    ] {
        let (status, body) = problem(refusal).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let field = |name: &str| body.get(name).and_then(Value::as_str).map(str::to_owned);
        assert_eq!(field("detail").as_deref(), Some(TEXT_TOO_LONG));
        assert_eq!(field("error_code").as_deref(), Some("UZ-REQ-004"));
        assert_eq!(
            field("user_message").as_deref(),
            Some("Input exceeds the maximum length of 1,048,576 characters.")
        );
    }
}

/// The ceiling itself is a message this surface takes, far past the 32,768
/// bytes it once refused.
#[test]
fn a_message_at_the_ceiling_is_read_whole() {
    let full: String = std::iter::repeat_n(WIDE, TEXT_MAX_CHARS).collect();
    let read = send(&message_body(&full)).expect("a message at the ceiling is read");
    assert_eq!(read.message.chars().count(), TEXT_MAX_CHARS);
    let created = create(&create_body(&full)).expect("a first message at the ceiling is read");
    assert_eq!(
        created.message.map(|message| message.chars().count()),
        Some(TEXT_MAX_CHARS)
    );
}

/// An empty message and one holding NUL break other rules on the same field,
/// so neither is told it is too long.
#[tokio::test]
async fn an_empty_or_nul_message_is_refused_without_the_ceiling_sentence() {
    for message in ["", "before\0after"] {
        let (status, detail) = answer(send(&message_body(message)).expect_err("refused")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{message:?}");
        assert_eq!(detail, INVALID, "{message:?}");
        let (_status, detail) = answer(create(&create_body(message)).expect_err("refused")).await;
        assert_eq!(detail, INVALID, "{message:?}");
    }
}

/// A NUL past the ceiling is told about its shape, the rule a person reading
/// the sentence can still break after shortening it.
#[tokio::test]
async fn a_long_message_holding_nul_is_told_its_shape() {
    let mut past: String = std::iter::repeat_n(WIDE, TEXT_MAX_CHARS).collect();
    past.push('\0');
    let (_status, detail) = answer(send(&message_body(&past)).expect_err("refused")).await;
    assert_eq!(detail, INVALID);
}
