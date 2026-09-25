//! Which chunk frames earn first-visible timing, and how they publish.

#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "the serialization fixture should fail loudly on a missing field"
)]

use std::borrow::Cow;

use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_wire::activity::{ActivityFrame, FleetResponseChunk};

use super::{Published, Target, first_visible_candidate_ms};

#[test]
fn first_chunk_timing_excludes_frames_a_viewer_must_suppress() {
    let chunk = |stream_start, stream_contiguous, stream_seq| {
        ActivityFrame::FleetResponseChunk(FleetResponseChunk {
            text: Cow::Borrowed("answer"),
            text_kind: Some(afd_wire::activity::StreamTextKind::Answer),
            first_chunk_after_ms: Some(42),
            stream_start,
            stream_contiguous,
            stream_seq,
        })
    };
    let dropped_first = chunk(false, false, 1);
    let repeated_start = chunk(true, true, 1);
    let valid = chunk(true, true, 0);
    let old_runner = ActivityFrame::FleetResponseChunk(FleetResponseChunk {
        text: Cow::Borrowed("old"),
        text_kind: None,
        first_chunk_after_ms: Some(42),
        stream_start: true,
        stream_contiguous: true,
        stream_seq: 0,
    });
    assert_eq!(first_visible_candidate_ms(&[dropped_first]), None);
    assert_eq!(first_visible_candidate_ms(&[repeated_start]), None);
    assert_eq!(first_visible_candidate_ms(&[valid]), Some(42));
    assert_eq!(first_visible_candidate_ms(&[old_runner]), None);
}

#[test]
fn first_chunk_marker_distinguishes_the_only_safe_stream_entry() {
    const FIXTURE_MILLIS: i64 = 1_000;
    let target = Target {
        fleet_id: Uuid7::encode(UnixMillis::from_millis(FIXTURE_MILLIS), [0; ENTROPY_LEN])
            .expect("fixture fleet id"),
        event_id: "event".to_owned(),
        lease_created_at: 0,
        event_created_at: 0,
        timing_eligible: true,
    };
    for (first_chunk_after_ms, stream_start, stream_contiguous, stream_seq) in [
        (Some(42), true, true, 0),
        (None, false, true, 1),
        (Some(50), false, false, 1),
    ] {
        let frame = ActivityFrame::FleetResponseChunk(FleetResponseChunk {
            text: Cow::Borrowed("answer"),
            text_kind: Some(afd_wire::activity::StreamTextKind::Answer),
            first_chunk_after_ms,
            stream_start,
            stream_contiguous,
            stream_seq,
        });
        let published = Published::of(&target, &frame).expect("chunk has no fallible fields");
        let value = serde_json::to_value(published).expect("published chunk serializes");
        assert_eq!(value["kind"], "chunk");
        assert_eq!(value["stream_start"], stream_start);
        assert_eq!(value["stream_contiguous"], stream_contiguous);
        assert_eq!(value["stream_seq"], stream_seq);
        assert_eq!(value["text"], "answer");
        assert_eq!(value["text_kind"], "answer");
    }
    let expired = Target {
        timing_eligible: false,
        ..target
    };
    let frame = ActivityFrame::FleetResponseChunk(FleetResponseChunk {
        text: Cow::Borrowed("stale"),
        text_kind: None,
        first_chunk_after_ms: Some(42),
        stream_start: true,
        stream_contiguous: true,
        stream_seq: 0,
    });
    let published = Published::of(&expired, &frame).expect("chunk has no fallible fields");
    let value = serde_json::to_value(published).expect("published chunk serializes");
    assert_eq!(value["stream_start"], false);
    assert_eq!(value["stream_contiguous"], false);
    assert!(value.get("text_kind").is_none());
}
