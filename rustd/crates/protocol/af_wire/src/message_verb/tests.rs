//! The messages verb's body: what the runner and the daemon both refuse.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::borrow::Cow;

use garde::Validate as _;

use super::{MessagePosted, MessageRequest};

/// The item every request here names.
const ITEM: &str = "0199a0b0-0000-7000-8000-0000000000aa";

/// A request carrying `text`.
fn request(text: String) -> MessageRequest<'static> {
    MessageRequest {
        fencing_token: 1,
        item_id: Cow::Borrowed(ITEM),
        call_number: 1,
        text: Cow::Owned(text),
    }
}

/// No length bound: a line far past the old 4,096 bytes passes, and the
/// channel decides whether it can take it.
#[test]
fn a_long_message_passes() {
    request("a".repeat(10_000))
        .validate()
        .expect("a 10,000-byte message passes");
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
    let body = r#"{"fencing_token":1,"item_id":"0199a0b0-0000-7000-8000-0000000000aa","call_number":1,"text":"hi","channel":"C1"}"#;
    let _refused =
        serde_json::from_str::<MessageRequest<'_>>(body).expect_err("the body is refused");
}

#[test]
fn the_reply_says_whether_the_thread_has_it() {
    let written = serde_json::to_string(&MessagePosted { delivered: true }).expect("serializes");
    assert_eq!(written, r#"{"delivered":true}"#);
}
