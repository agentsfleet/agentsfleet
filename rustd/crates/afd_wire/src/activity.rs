//! The live-tail progress frames a run streams while it works.
//!
//! Ephemeral and best-effort: a dropped frame is cosmetic. The durable system of
//! record is the report. Arguments are redacted runner-side before they reach
//! this type, so resolved secret bytes never cross this boundary.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

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
