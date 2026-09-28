//! What a steer's body is allowed to be, and how a failed steer is answered.
//!
//! The write decides whether the bytes a client sent are a message at all
//! before any datastore is reached, so it is proven here;
//! `fleet_messages_steer.rs` proves the refusal survives the layer stack.

#![expect(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use afd_wire::event::STEER_MESSAGE_MAX_BYTES;
use axum::body::Bytes;
use axum::response::IntoResponse as _;
use http::StatusCode;

use super::{read_steer, refuse_steer};

/// Only a reused operation id is answered as a 409 naming the id's state; every
/// other steer failure keeps the status its plane decided.
///
/// The claim is that the two arms stay apart: a datastore outage rendered as
/// "admitted" would tell a caller to stop retrying a message that never landed.
#[test]
fn only_a_reused_id_is_answered_as_admitted() {
    for (label, error) in afd_events::error::one_of_each_kind() {
        let conflict = error.is_operation_conflict();
        let status = refuse_steer(error).into_response().status();
        assert_eq!(
            status == StatusCode::CONFLICT,
            conflict,
            "{label} answered {status}"
        );
    }
}

/// A steer with nothing in it is refused before the parser runs.
#[test]
fn should_refuse_a_steer_that_carries_no_body() {
    read_steer(&Bytes::new()).unwrap_err();
}

/// A body this daemon cannot read is refused.
#[test]
fn should_refuse_a_body_that_is_not_a_message() {
    for body in [
        "",
        "{",
        "null",
        "[]",
        r#""hello""#,
        "{}",
        r#"{"message":null}"#,
        r#"{"message":7}"#,
    ] {
        assert!(
            read_steer(&Bytes::from(body.as_bytes().to_vec())).is_err(),
            "{body} is not a steer this surface accepts"
        );
    }
}

/// An empty message is refused: a person pressed send on nothing.
#[test]
fn should_refuse_an_empty_message() {
    read_steer(&Bytes::from_static(br#"{"message":""}"#)).unwrap_err();
}

/// An escaped message is a message, not a malformed body.
///
/// The regression this file exists for on the write side. `serde` hands back
/// `Cow::Owned` for any string carrying an escape, so a reader that accepted
/// only a borrow would refuse a newline, a quote and an emoji — which is most
/// of what a person types into a chat box.
#[test]
fn should_read_a_message_that_carries_escapes() {
    let body = Bytes::from_static(br#"{"message":"line one\nline \"two\"\tand \u2728 done"}"#);
    assert_eq!(
        read_steer(&body).unwrap().message,
        "line one\nline \"two\"\tand \u{2728} done",
    );
}

/// The length bound is on the DECODED bytes, not on what a client sent.
///
/// A message of newlines doubles in the encoded form: bounding the escaped
/// bytes would refuse a message half the documented size, and the runner reads
/// the decoded text.
#[test]
fn should_bound_the_decoded_bytes_and_not_the_escaped_ones() {
    let escaped = format!(
        r#"{{"message":"{}"}}"#,
        r"\n".repeat(STEER_MESSAGE_MAX_BYTES)
    );
    let body = Bytes::from(escaped.into_bytes());
    let read = read_steer(&body).expect("a message of newlines is under the bound once decoded");
    assert_eq!(read.message.len(), STEER_MESSAGE_MAX_BYTES);
}

/// The bound admits its own ceiling and refuses one byte past it.
#[test]
fn should_admit_the_ceiling_and_refuse_one_byte_past_it() {
    let at_the_ceiling = format!(r#"{{"message":"{}"}}"#, "a".repeat(STEER_MESSAGE_MAX_BYTES));
    read_steer(&Bytes::from(at_the_ceiling.into_bytes()))
        .expect("the documented ceiling is a message this surface takes");

    let one_past = format!(
        r#"{{"message":"{}"}}"#,
        "a".repeat(STEER_MESSAGE_MAX_BYTES + 1)
    );
    read_steer(&Bytes::from(one_past.into_bytes())).unwrap_err();
}
