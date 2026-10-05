//! The messages verb's body: the bound the runner and the daemon share.

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
    assert!(request("a".repeat(MESSAGE_MAX_BYTES)).validate().is_ok());
    assert!(
        request("a".repeat(MESSAGE_MAX_BYTES + 1))
            .validate()
            .is_err()
    );
}

#[test]
fn an_empty_message_is_refused() {
    assert!(request(String::new()).validate().is_err());
}

/// Postgres cannot store a NUL, and a thread cannot render one.
#[test]
fn a_message_holding_a_nul_is_refused() {
    assert!(request("before\0after".to_owned()).validate().is_err());
}

#[test]
fn a_message_naming_anything_else_is_refused() {
    let body = r#"{"fencing_token":1,"text":"hi","channel":"C1"}"#;
    assert!(serde_json::from_str::<MessageRequest<'_>>(body).is_err());
}

#[test]
fn the_reply_says_whether_the_thread_has_it() {
    let written = serde_json::to_string(&MessagePosted { delivered: true }).expect("serializes");
    assert_eq!(written, r#"{"delivered":true}"#);
}
