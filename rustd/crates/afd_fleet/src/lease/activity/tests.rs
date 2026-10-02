//! Which chunk frames earn first-visible timing, and how they publish.

#![expect(
    clippy::expect_used,
    clippy::indexing_slicing,
    reason = "the serialization fixture should fail loudly on a missing field"
)]

use std::borrow::Cow;

use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_wire::activity::{
    ActivityFrame, FleetResponseChunk, ToolCallCompleted, ToolCallProgress, ToolCallStarted,
};

use super::published::fenced_call_id;
use super::{Published, Target, first_visible_candidate_ms};

/// The fencing token the fixture lease holds.
const FENCE: i64 = 7;

/// The call id the runner stamps on every fixture tool frame.
const RUNNER_CALL: &str = "3";

/// The published stream route whose description lists the frame kinds.
const STREAM_DESCRIPTION: &str =
    "/paths/~1v1~1workspaces~1{workspace_id}~1fleets~1{fleet_id}~1events~1stream/get/description";

/// The tool every fixture frame names.
const TOOL: &str = "shell";

/// A lease holder's target, eligible for timing.
fn target() -> Target {
    const FIXTURE_MILLIS: i64 = 1_000;
    Target {
        fleet_id: Uuid7::encode(UnixMillis::from_millis(FIXTURE_MILLIS), [0; ENTROPY_LEN])
            .expect("fixture fleet id"),
        event_id: "event".to_owned(),
        lease_created_at: 0,
        event_created_at: 0,
        timing_eligible: true,
        fence: FENCE,
    }
}

/// One frame of every kind, each tool frame naming `call_id`.
fn every_frame(call_id: Option<&'static str>) -> [ActivityFrame<'static>; 4] {
    let call_id = call_id.map(Cow::Borrowed);
    [
        ActivityFrame::ToolCallStarted(ToolCallStarted {
            name: Cow::Borrowed(TOOL),
            args_redacted: Cow::Borrowed("{}"),
            call_id: call_id.clone(),
        }),
        ActivityFrame::ToolCallProgress(ToolCallProgress {
            name: Cow::Borrowed(TOOL),
            elapsed_ms: 1,
            call_id: call_id.clone(),
        }),
        ActivityFrame::ToolCallCompleted(ToolCallCompleted {
            name: Cow::Borrowed(TOOL),
            ms: 2,
            call_id,
            status: None,
            output_head: None,
            output_tail: None,
            output_line_count: None,
            exit_code: None,
        }),
        ActivityFrame::FleetResponseChunk(FleetResponseChunk {
            text: Cow::Borrowed("answer"),
            text_kind: None,
            first_chunk_after_ms: None,
            stream_start: false,
            stream_contiguous: false,
            stream_seq: 0,
        }),
    ]
}

fn published(frame: &ActivityFrame<'_>) -> serde_json::Value {
    let target = target();
    let published = Published::of(&target, frame).expect("every fixture frame renders");
    serde_json::to_value(published).expect("a published frame serializes")
}

#[test]
fn tool_frames_republish_their_call_id_and_never_invent_one() {
    let fenced = serde_json::Value::from(format!("{FENCE}:{RUNNER_CALL}"));
    for frame in every_frame(Some(RUNNER_CALL)) {
        let value = published(&frame);
        let expected = frame.call_id().map(|_named| fenced.clone());
        assert_eq!(value.get("call_id").cloned(), expected, "{value}");
    }
    for frame in every_frame(None) {
        assert!(published(&frame).get("call_id").is_none());
    }
}

/// A reclaimed lease re-runs the same event and the runner's counter restarts,
/// so its call 3 must not publish as the dead lease's call 3.
#[test]
fn one_runner_call_id_under_two_fences_publishes_two_ids() {
    let reclaimed = Target {
        fence: FENCE + 1,
        ..target()
    };
    let [started, ..] = every_frame(Some(RUNNER_CALL));
    let first = published(&started);
    let second = serde_json::to_value(
        Published::of(&reclaimed, &started).expect("every fixture frame renders"),
    )
    .expect("a published frame serializes");
    assert_ne!(first["call_id"], second["call_id"], "{first} / {second}");
    assert_eq!(
        second["call_id"],
        serde_json::Value::from(format!("{}:{RUNNER_CALL}", FENCE + 1))
    );
    // A runner id carrying the separator still reads back past the first one.
    assert_eq!(fenced_call_id(FENCE, Some("a:b")).as_deref(), Some("7:a:b"));
}

#[test]
fn test_sse_kind_list_matches_published_kinds() {
    let openapi = include_str!("../../../../../../public/openapi.json");
    let document: serde_json::Value =
        serde_json::from_str(openapi).expect("the published spec parses");
    let description = document
        .pointer(STREAM_DESCRIPTION)
        .and_then(serde_json::Value::as_str)
        .expect("the stream route is described");
    for frame in every_frame(Some(RUNNER_CALL)) {
        let value = published(&frame);
        let kind = value["kind"].as_str().expect("every frame has a kind");
        assert!(
            description.contains(&format!("`{kind}`")),
            "`{kind}` is undocumented"
        );
    }
    assert!(
        !description.contains("`fleet_response_chunk`"),
        "the stream publishes `chunk`, never the runner's own name for it"
    );
    assert!(
        description.contains("`call_id`"),
        "the tool frames' call id is documented"
    );
}

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
    let target = target();
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

#[test]
fn test_published_completed_carries_outcome() {
    use afd_wire::tool_trace::ToolCallStatus;
    let frame = ActivityFrame::ToolCallCompleted(ToolCallCompleted {
        name: Cow::Borrowed(TOOL),
        ms: 2,
        call_id: Some(Cow::Borrowed(RUNNER_CALL)),
        status: Some(ToolCallStatus::Failed),
        output_head: Some(Cow::Borrowed("error: denied")),
        output_tail: Some(Cow::Borrowed("exit")),
        output_line_count: Some(40),
        exit_code: Some(2),
    });
    let value = published(&frame);
    assert_eq!(value["kind"], "tool_call_completed");
    assert_eq!(value["status"], "failed");
    assert_eq!(value["output_head"], "error: denied");
    assert_eq!(value["output_tail"], "exit");
    assert_eq!(value["output_line_count"], 40);
    assert_eq!(value["exit_code"], 2);

    let [.., completed, _chunk] = every_frame(None);
    let bare = published(&completed);
    for absent in [
        "status",
        "output_head",
        "output_tail",
        "output_line_count",
        "exit_code",
    ] {
        assert!(
            bare.get(absent).is_none(),
            "no {absent} is invented: {bare}"
        );
    }
}
