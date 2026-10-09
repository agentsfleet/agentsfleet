#![expect(
    clippy::expect_used,
    reason = "a test asserts by panicking; the manifest's restriction set is for the daemon"
)]

use std::borrow::Cow;

use super::{ActivityAccepted, ActivityFrame, FleetResponseChunk, StreamTextKind};

/// The wire chunk a runner sends, with only the fields a case varies named.
fn chunk(text: &'static str, text_kind: Option<StreamTextKind>) -> ActivityFrame<'static> {
    ActivityFrame::FleetResponseChunk(FleetResponseChunk {
        text: Cow::Borrowed(text),
        text_kind,
        first_chunk_after_ms: None,
        stream_start: false,
        stream_contiguous: false,
        stream_seq: 0,
    })
}

/// A tool frame's call id is optional and bounded; a chunk never names one,
/// and any other unknown field still refuses the frame.
#[test]
fn a_tool_frame_names_its_call_within_the_bound() {
    use super::{CALL_ID_MAX_BYTES, ToolCallCompleted};
    let completed = |call_id: Option<String>| {
        ActivityFrame::ToolCallCompleted(ToolCallCompleted {
            call_id: call_id.map(Cow::Owned),
            ..completed_frame()
        })
    };
    assert!(completed(None).call_id_usable());
    assert!(completed(Some("c".repeat(CALL_ID_MAX_BYTES))).call_id_usable());
    assert!(!completed(Some(String::new())).call_id_usable());
    assert!(!completed(Some("c".repeat(CALL_ID_MAX_BYTES + 1))).call_id_usable());
    assert!(chunk("text", None).call_id_usable());
    assert_eq!(chunk("text", None).call_id(), None);

    let named: ActivityFrame<'_> = serde_json::from_str(
        r#"{"tool_call_progress":{"name":"shell","elapsed_ms":1,"call_id":"7"}}"#,
    )
    .expect("a named frame parses");
    assert_eq!(named.call_id(), Some("7"));
    let foreign = serde_json::from_str::<ActivityFrame<'_>>(
        r#"{"tool_call_progress":{"name":"shell","elapsed_ms":1,"retries":1}}"#,
    );
    assert!(foreign.is_err(), "any other unknown field is still refused");
}

/// Every tool frame bounds its call id on its own type, as a runner posts it:
/// empty and 65 bytes are malformed, 64 bytes is a call.
#[test]
fn test_activity_frame_call_id_is_bounded() {
    use super::CALL_ID_MAX_BYTES;
    let frames = |call_id: &str| {
        [
            format!(
                r#"{{"tool_call_started":{{"name":"shell","args_redacted":"{{}}","call_id":"{call_id}"}}}}"#
            ),
            format!(r#"{{"tool_call_completed":{{"name":"shell","ms":1,"call_id":"{call_id}"}}}}"#),
            format!(
                r#"{{"tool_call_progress":{{"name":"shell","elapsed_ms":1,"call_id":"{call_id}"}}}}"#
            ),
        ]
    };
    let usable = |call_id: &str| {
        frames(call_id).map(|body| {
            serde_json::from_str::<ActivityFrame<'_>>(&body)
                .expect("a tool frame parses")
                .call_id_usable()
        })
    };
    assert_eq!(usable(""), [false; 3]);
    assert_eq!(usable(&"c".repeat(CALL_ID_MAX_BYTES + 1)), [false; 3]);
    assert_eq!(usable(&"c".repeat(CALL_ID_MAX_BYTES)), [true; 3]);
}

/// Each tool frame's published `call_id` description states the bound
/// `call_id_usable` enforces, so the two cannot drift apart unnoticed.
#[test]
fn a_published_call_id_description_names_its_bound() {
    use super::CALL_ID_MAX_BYTES;
    let openapi = include_str!("../../../../../public/openapi.json");
    let document: serde_json::Value =
        serde_json::from_str(openapi).expect("the published spec parses");
    let bound = format!("1 to {CALL_ID_MAX_BYTES} bytes");
    for frame in ["ToolCallStarted", "ToolCallProgress", "ToolCallCompleted"] {
        let pointer = format!("/components/schemas/{frame}/properties/call_id/description");
        let text = document
            .pointer(&pointer)
            .and_then(serde_json::Value::as_str)
            .map(|text| text.split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        assert!(text.contains(&bound), "{frame}: {text}");
    }
}

/// The acknowledgement is exactly the one field `ActivityAccepted` declares.
#[test]
fn test_the_acknowledgement_is_exactly_ok_true() {
    assert_eq!(
        serde_json::to_string(&ActivityAccepted { ok: true })
            .ok()
            .as_deref(),
        Some(r#"{"ok":true}"#),
    );
}

#[test]
fn typed_chunks_round_trip_and_old_runner_chunks_stay_untyped() {
    // Whole-frame equality, not a destructure: it holds the variant and
    // every defaulted field to the same standard as the one field the case
    // is named for, and it leaves no unreachable `else` arm behind.
    let typed: ActivityFrame<'_> = serde_json::from_str(
        r#"{"fleet_response_chunk":{"text":"thought","text_kind":"reasoning"}}"#,
    )
    .expect("typed runner chunk decodes");
    assert_eq!(typed, chunk("thought", Some(StreamTextKind::Reasoning)));
    let old: ActivityFrame<'_> =
        serde_json::from_str(r#"{"fleet_response_chunk":{"text":"legacy"}}"#)
            .expect("old runner chunk decodes");
    assert_eq!(old, chunk("legacy", None));
}

/// A completion with no outcome, as a runner that reports none sends it.
fn completed_frame() -> super::ToolCallCompleted<'static> {
    super::ToolCallCompleted {
        name: Cow::Borrowed("shell"),
        ms: 1,
        call_id: None,
        status: None,
        output_head: None,
        output_tail: None,
        output_line_count: None,
        exit_code: None,
    }
}

#[test]
fn test_tool_call_completed_outcome_roundtrip() {
    use crate::tool_trace::ToolCallStatus;
    for status in [
        ToolCallStatus::Succeeded,
        ToolCallStatus::Failed,
        ToolCallStatus::Interrupted,
    ] {
        for exit_code in [None, Some(2)] {
            let sent = ActivityFrame::ToolCallCompleted(super::ToolCallCompleted {
                call_id: Some(Cow::Borrowed("3")),
                status: Some(status),
                output_head: Some(Cow::Borrowed("# agentsfleet\nline two")),
                output_tail: Some(Cow::Borrowed("MIT")),
                output_line_count: Some(214),
                exit_code,
                ..completed_frame()
            });
            let text = serde_json::to_string(&sent).expect("encodes");
            let back: ActivityFrame<'_> = serde_json::from_str(&text).expect("decodes");
            assert_eq!(back, sent, "{text}");
        }
    }
}

#[test]
fn test_tool_call_completed_without_outcome_parses() {
    let old: ActivityFrame<'_> =
        serde_json::from_str(r#"{"tool_call_completed":{"name":"shell","ms":1}}"#)
            .expect("a frame from a runner that reports no outcome parses");
    assert_eq!(old, ActivityFrame::ToolCallCompleted(completed_frame()));
    assert_eq!(
        serde_json::to_string(&old).ok().as_deref(),
        Some(r#"{"tool_call_completed":{"name":"shell","ms":1}}"#),
        "and re-encodes without the absent fields"
    );
}

#[test]
fn a_completion_edge_past_its_bound_is_unusable() {
    use crate::tool_trace::OUTPUT_EDGE_MAX_BYTES;
    let with_tail = |tail: String| {
        ActivityFrame::ToolCallCompleted(super::ToolCallCompleted {
            output_tail: Some(Cow::Owned(tail)),
            ..completed_frame()
        })
    };
    assert!(ActivityFrame::ToolCallCompleted(completed_frame()).outcome_usable());
    assert!(with_tail("a".repeat(OUTPUT_EDGE_MAX_BYTES)).outcome_usable());
    assert!(!with_tail("a".repeat(OUTPUT_EDGE_MAX_BYTES + 1)).outcome_usable());
    let head = ActivityFrame::ToolCallCompleted(super::ToolCallCompleted {
        output_head: Some(Cow::Borrowed("1\n2\n3\n4\n5\n6")),
        ..completed_frame()
    });
    assert!(!head.outcome_usable(), "six lines");
    assert!(
        chunk("text", None).outcome_usable(),
        "a chunk carries no edges"
    );
}
