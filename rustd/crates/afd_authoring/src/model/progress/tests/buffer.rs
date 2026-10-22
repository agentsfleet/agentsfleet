//! Public preview invariants: Unicode bounds, immediate first text and visible gaps.

#![expect(
    clippy::expect_used,
    reason = "fixture construction and queue or validation outcomes must fail this proof if they change"
)]

use super::super::{CAPACITY, DELTA_BYTES, Delta, Preview, TextKind};
use afd_core::id::Uuid7;
use tokio::sync::mpsc;

#[test]
fn first_text_is_immediate_and_later_text_is_coalesced() {
    let (sender, mut reader) = mpsc::channel(CAPACITY);
    let mut preview = Preview::new(sender);
    preview.flush();
    preview.push("");
    assert_eq!(
        reader.try_recv().err(),
        Some(mpsc::error::TryRecvError::Empty)
    );
    preview.push("first");
    let first = reader.try_recv().expect("first text requires no timer");
    assert_eq!(first.sequence, 1);
    assert_eq!(&*first.text, "first");
    preview.push("second");
    preview.push(" + third");
    assert_eq!(
        reader.try_recv().err(),
        Some(mpsc::error::TryRecvError::Empty)
    );
    preview.flush();
    let later = reader.try_recv().expect("later text is coalesced");
    assert_eq!(later.sequence, 2);
    assert_eq!(&*later.text, "second + third");
}

#[test]
fn multibyte_text_remains_complete_and_every_frame_fits_the_wire_bound() {
    let (sender, mut reader) = mpsc::channel(CAPACITY);
    let mut preview = Preview::new(sender);
    let text = "a🦉".repeat(DELTA_BYTES);
    preview.push(&text);
    preview.flush();
    let mut received = String::new();
    let mut sequence = 0;
    while let Ok(piece) = reader.try_recv() {
        sequence += 1;
        assert_eq!(piece.sequence, sequence);
        assert!(!piece.text.is_empty());
        assert!(piece.text.len() <= DELTA_BYTES);
        received.push_str(&piece.text);
    }
    assert_eq!(received, text);
}

#[test]
fn saturated_preview_never_blocks_and_the_next_frame_reveals_the_gap() {
    let (sender, mut reader) = mpsc::channel(CAPACITY);
    let mut preview = Preview::new(sender);
    for _ in 0..=CAPACITY {
        preview.push("answer");
        preview.flush();
    }
    for sequence in 1..=CAPACITY {
        assert_eq!(
            reader.try_recv().expect("bounded queue").sequence,
            sequence as u64
        );
    }
    assert_eq!(
        reader.try_recv().err(),
        Some(mpsc::error::TryRecvError::Empty)
    );
    preview.push("recovered");
    preview.flush();
    let next = reader
        .try_recv()
        .expect("publication can resume after a gap");
    assert_eq!(next.sequence, CAPACITY as u64 + 2);
    assert_eq!(&*next.text, "recovered");
    drop(reader);
    preview.push("after revocation");
    preview.flush();
}

fn frame(text: String) -> Delta {
    let id = Uuid7::parse("01990000-0000-7000-8000-000000000001").expect("fixture UUIDv7");
    Delta {
        draft_id: id.clone(),
        sequence: "1".into(),
        stream_id: id,
        stream_seq: 1,
        stream_start: true,
        text_kind: TextKind::Answer,
        text,
    }
}

#[test]
fn preview_debug_never_exposes_private_text() {
    let preview = frame("never log this private answer".into());
    let logged = format!("{preview:?}");
    assert!(!logged.contains(&preview.text));
    assert!(logged.contains("text_bytes: 29"));
    assert!(logged.contains("stream_seq: 1"));
}

#[test]
fn wire_bounds_count_bytes_and_reject_unordered_or_empty_frames() {
    garde::Unvalidated::new(frame("🦉".repeat(DELTA_BYTES / 4)))
        .validate()
        .expect("Unicode text at the byte ceiling is valid");
    garde::Unvalidated::new(frame("🦉".repeat(DELTA_BYTES / 4 + 1)))
        .validate()
        .expect_err("one more Unicode character exceeds the byte ceiling");
    garde::Unvalidated::new(frame(String::new()))
        .validate()
        .expect_err("empty previews are invalid");
    let mut invalid = frame("answer".into());
    invalid.stream_seq = 0;
    garde::Unvalidated::new(invalid)
        .validate()
        .expect_err("stream positions start at one");
    let mut invalid = frame("answer".into());
    invalid.sequence = "not a number".into();
    garde::Unvalidated::new(invalid)
        .validate()
        .expect_err("durable message positions contain only digits");
}
