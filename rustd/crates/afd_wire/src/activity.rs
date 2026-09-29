//! The live-tail progress frames a run streams while it works.
//!
//! Ephemeral and best-effort: a dropped frame is cosmetic. The durable system of
//! record is the report. Arguments are redacted runner-side before they reach
//! this type, so resolved secret bytes never cross this boundary.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// The longest call identity a tool frame may carry.
///
/// A runner's own counter needs a few bytes. The bound leaves room for a
/// provider's call id, should a runner forward one, and keeps one frame from
/// deciding how much of every subscriber's buffer an identity takes. The three
/// `call_id` descriptions spell this number out, because they are published
/// and a constant's name means nothing to a runner's author.
pub const CALL_ID_MAX_BYTES: usize = 64;

/// A tool call began.
///
/// `args_redacted` is opaque, pre-stringified JSON built runner-side AFTER
/// substitution — never the resolved bytes.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCallStarted<'a> {
    /// Which tool.
    #[serde(borrow)]
    pub name: Cow<'a, str>,
    /// The redacted arguments.
    #[serde(borrow)]
    pub args_redacted: Cow<'a, str>,
    /// Which call of the run this frame belongs to, 1 to 64 bytes: every frame
    /// of one call carries the same value. Absent from runners that do not name
    /// calls.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<Cow<'a, str>>,
}

/// Which part of the model's output a streamed chunk carries: the answer, or
/// the model's reasoning before it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamTextKind {
    /// User-visible answer text from a provider field.
    Answer,
    /// Reasoning text from a dedicated provider field.
    Reasoning,
}

/// The fleet produced output.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FleetResponseChunk<'a> {
    /// The text produced.
    #[serde(borrow)]
    pub text: Cow<'a, str>,
    /// Absent on older runners; clients use the saved final reply then.
    #[serde(default)]
    pub text_kind: Option<StreamTextKind>,
    /// Present only on the first safe output chunk; relative to agent runtime invocation.
    #[serde(default)]
    pub first_chunk_after_ms: Option<u64>,
    /// True only for the first delivered chunk of an intact model pass.
    #[serde(default)]
    pub stream_start: bool,
    /// False once a runner lost an earlier chunk in this model pass.
    #[serde(default)]
    pub stream_contiguous: bool,
    /// Zero-based output position for detecting loss after the child pipe.
    #[serde(default)]
    pub stream_seq: u64,
}

/// A tool call finished.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCallCompleted<'a> {
    /// Which tool.
    #[serde(borrow)]
    pub name: Cow<'a, str>,
    /// How long it took, in milliseconds.
    pub ms: i64,
    /// Which call of the run this frame belongs to, 1 to 64 bytes: every frame
    /// of one call carries the same value. Absent from runners that do not name
    /// calls.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<Cow<'a, str>>,
}

/// A long-running tool is still working, so a reader's spinner survives it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCallProgress<'a> {
    /// Which tool.
    #[serde(borrow)]
    pub name: Cow<'a, str>,
    /// How long it has been running, in milliseconds.
    pub elapsed_ms: i64,
    /// Which call of the run this frame belongs to, 1 to 64 bytes: every frame
    /// of one call carries the same value. Absent from runners that do not name
    /// calls.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<Cow<'a, str>>,
}

/// One progress frame.
//
// The variant name IS the wire discriminator, so the enum is the single source
// for the vocabulary and there are no re-spelled kind strings. Each payload is
// a named struct rather than an inline variant body, matching the Zig union
// field for field — the encoding is identical either way, and the named form
// is what lets each payload carry its own fixture.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityFrame<'a> {
    /// A tool call began.
    #[serde(borrow)]
    ToolCallStarted(ToolCallStarted<'a>),
    /// The fleet produced output.
    #[serde(borrow)]
    FleetResponseChunk(FleetResponseChunk<'a>),
    /// A tool call finished.
    #[serde(borrow)]
    ToolCallCompleted(ToolCallCompleted<'a>),
    /// A long-running tool is still working.
    #[serde(borrow)]
    ToolCallProgress(ToolCallProgress<'a>),
}

impl ActivityFrame<'_> {
    /// The call a tool frame names, if it names one.
    #[must_use]
    pub fn call_id(&self) -> Option<&str> {
        match self {
            Self::ToolCallStarted(body) => body.call_id.as_deref(),
            Self::ToolCallProgress(body) => body.call_id.as_deref(),
            Self::ToolCallCompleted(body) => body.call_id.as_deref(),
            Self::FleetResponseChunk(_) => None,
        }
    }

    /// Whether the call this frame names, if any, is 1 to
    /// [`CALL_ID_MAX_BYTES`] bytes.
    #[must_use]
    pub fn call_id_usable(&self) -> bool {
        self.call_id()
            .is_none_or(|id| (1..=CALL_ID_MAX_BYTES).contains(&id.len()))
    }
}

/// `POST /v1/runners/me/leases/{lease_id}/activity` request, a batch of frames.
///
/// One frame per request today; the array shape lets a later change coalesce
/// without a wire change. The reply is `202` carrying [`ActivityAccepted`].
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityRequest<'a> {
    /// The frames being forwarded.
    #[serde(borrow)]
    pub frames: Vec<ActivityFrame<'a>>,
}

/// The acknowledgement a batch of frames is answered with.
///
/// `202`, because the frames were received and not yet read by anybody. The
/// publish is best-effort and a subscriber seeing a frame is a separate event
/// from this call returning.
// The one field `service_activity.zig` answers, so a runner pointed at either
// daemon reads one shape. The first port of the verb dropped it and answered
// a bare status; the document gate is what noticed.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityAccepted {
    /// Always `true`: a batch this daemon could not accept is refused with a
    /// problem document, never acknowledged with `false`.
    pub ok: bool,
}

#[cfg(test)]
mod tests {
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
                name: Cow::Borrowed("shell"),
                ms: 1,
                call_id: call_id.map(Cow::Owned),
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

    /// Each tool frame's published `call_id` description states the bound
    /// `call_id_usable` enforces, so the two cannot drift apart unnoticed.
    #[test]
    fn a_published_call_id_description_names_its_bound() {
        use super::CALL_ID_MAX_BYTES;
        let openapi = include_str!("../../../../public/openapi.json");
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

    /// The acknowledgement is the one field `service_activity.zig` writes.
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
}
