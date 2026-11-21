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

/// The bound counts the text as the frame writes it: a control character is a
/// six-byte escape and a quote a two-byte one, so a head of them is cut
/// sooner, and the frame stays within the bound plus its envelope.
#[test]
fn frame_text_counts_the_escaped_bytes_the_frame_carries() {
    let bound = 64 * 1024;
    let controls = "\u{1}".repeat(bound);
    let cut = FrameText::within(&controls, bound);
    assert!(cut.trimmed);
    assert_eq!(cut.text.chars().count(), bound / 6);
    let written = serde_json::to_string(cut.text).expect("a string serializes");
    assert!(written.len() <= bound + 2, "{} bytes", written.len());
    let frame = serde_json::to_string(&admitted(cut.text, cut.trimmed)).expect("a frame");
    assert!(frame.len() <= 2 * bound, "{} bytes", frame.len());

    let quotes = FrameText::within("\"\"\"", 5);
    assert_eq!((quotes.text, quotes.trimmed), ("\"\"", true));
    let mixed = FrameText::within("a\u{1}b", 7);
    assert_eq!((mixed.text, mixed.trimmed), ("a\u{1}", true));
    let fits = FrameText::within("a\u{1}b", 8);
    assert_eq!((fits.text, fits.trimmed), ("a\u{1}b", false));
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
