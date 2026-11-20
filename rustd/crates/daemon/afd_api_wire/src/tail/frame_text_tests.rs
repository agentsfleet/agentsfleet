//! How much of a person's text one live frame carries, and how it says the
//! rest is on the turn.

#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::borrow::Cow;

use serde_json::{Value, json};

use super::{FrameText, TailFrame};

/// An admitted frame carrying `message`, marked `trimmed`.
fn admitted(message: &str, trimmed: bool) -> Value {
    let frame = TailFrame::TurnAdmitted {
        turn_id: Cow::Borrowed("1725000000000-3"),
        actor: Cow::Borrowed("steer:user_1"),
        event_type: Cow::Borrowed("chat"),
        message: Cow::Borrowed(message),
        message_trimmed: trimmed,
        created_at: jiff::Timestamp::constant(1_725_000_000, 0),
    };
    serde_json::to_value(&frame).expect("a frame serializes")
}

/// A text within the bound rides whole and unmarked; one past it rides as its
/// head, cut on a character boundary, and says so.
#[test]
fn frame_text_keeps_a_head_cut_on_a_character_boundary() {
    let whole = FrameText::within("check the tests", 64);
    assert_eq!((whole.text, whole.trimmed), ("check the tests", false));
    let wide = "é".repeat(10);
    let cut = FrameText::within(&wide, 5);
    assert_eq!((cut.text, cut.trimmed), ("éé", true));
    let exact = FrameText::within(&wide, 20);
    assert_eq!((exact.text.len(), exact.trimmed), (20, false));
    let empty = FrameText::within("", 0);
    assert_eq!((empty.text, empty.trimmed), ("", false));
}

/// Only a trimmed message names the marker: a whole one carries no key, so a
/// client that never learned the marker reads every frame it read before.
#[test]
fn only_a_trimmed_message_carries_the_marker() {
    let whole = admitted("check the tests", false);
    assert_eq!(whole.get("message_trimmed"), None, "{whole}");
    let trimmed = admitted("check the", true);
    assert_eq!(
        trimmed.get("message_trimmed"),
        Some(&json!(true)),
        "{trimmed}"
    );
    assert_eq!(
        trimmed.get("message"),
        Some(&json!("check the")),
        "{trimmed}"
    );
}
