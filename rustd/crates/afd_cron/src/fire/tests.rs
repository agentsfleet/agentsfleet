//! The body a fire stores, as the lease and the runner read it.

#![allow(
    clippy::expect_used,
    reason = "test target: an unparseable body should fail the test loudly"
)]

use super::{FIELD_MESSAGE, body};

/// The message, read back out of the body the way the runner reads it.
fn message_of(stored: &str) -> String {
    let value: serde_json::Value = serde_json::from_str(stored).expect("the body is JSON");
    value
        .get(FIELD_MESSAGE)
        .and_then(serde_json::Value::as_str)
        .expect("the body names a message")
        .to_owned()
}

#[test]
fn test_a_plain_message_is_stored_as_json() {
    let stored = body("weekly check");
    assert_eq!(stored, r#"{"message":"weekly check"}"#);
    assert_eq!(message_of(&stored), "weekly check");
}

#[test]
fn test_quotes_and_newlines_survive_the_round_trip() {
    let message = "re-check \"error rate\"\nthen post\tthe number";
    assert_eq!(message_of(&body(message)), message);
}

#[test]
fn test_a_message_that_is_already_json_is_kept_as_text() {
    let message = r#"{"message":"nested"}"#;
    assert_eq!(message_of(&body(message)), message);
}
