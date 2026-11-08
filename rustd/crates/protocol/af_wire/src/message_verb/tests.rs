//! The messages verb's body: the bound the runner and the daemon share.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::borrow::Cow;

use garde::Validate as _;

use super::{MESSAGE_MAX_BYTES, MessagePosted, MessageRequest};

/// A request carrying `text`.
fn request(text: String) -> MessageRequest<'static> {
    MessageRequest {
        fencing_token: 1,
        text: Cow::Owned(text),
    }
}

#[test]
fn a_message_at_the_cap_passes_and_one_byte_more_does_not() {
    request("a".repeat(MESSAGE_MAX_BYTES))
        .validate()
        .expect("a message at the cap passes");
    let _refused = request("a".repeat(MESSAGE_MAX_BYTES + 1))
        .validate()
        .expect_err("one byte more is refused");
}

#[test]
fn an_empty_message_is_refused() {
    let _refused = request(String::new())
        .validate()
        .expect_err("an empty message is refused");
}

/// Postgres cannot store a NUL, and a thread cannot render one.
#[test]
fn a_message_holding_a_nul_is_refused() {
    let _refused = request("before\0after".to_owned())
        .validate()
        .expect_err("a NUL is refused");
}

#[test]
fn a_message_naming_anything_else_is_refused() {
    let body = r#"{"fencing_token":1,"text":"hi","channel":"C1"}"#;
    let _refused =
        serde_json::from_str::<MessageRequest<'_>>(body).expect_err("the body is refused");
}

#[test]
fn the_reply_says_whether_the_thread_has_it() {
    let written = serde_json::to_string(&MessagePosted { delivered: true }).expect("serializes");
    assert_eq!(written, r#"{"delivered":true}"#);
}
