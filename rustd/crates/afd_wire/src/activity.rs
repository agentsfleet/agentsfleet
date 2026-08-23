//! The live-tail progress frames a run streams while it works.
//!
//! Ephemeral and best-effort: a dropped frame is cosmetic. The durable system of
//! record is the report. Arguments are redacted runner-side before they reach
//! this type, so resolved secret bytes never cross this boundary.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// One progress frame.
///
/// The variant name IS the wire discriminator, so the enum is the single source
/// for the vocabulary and there are no re-spelled kind strings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityFrame<'a> {
    /// A tool call began.
    ToolCallStarted {
        /// Which tool.
        #[serde(borrow)]
        name: Cow<'a, str>,
        /// Opaque, pre-stringified JSON built runner-side AFTER substitution —
        /// never the resolved bytes.
        #[serde(borrow)]
        args_redacted: Cow<'a, str>,
    },
    /// The fleet produced output.
    FleetResponseChunk {
        /// The text produced.
        #[serde(borrow)]
        text: Cow<'a, str>,
    },
    /// A tool call finished.
    ToolCallCompleted {
        /// Which tool.
        #[serde(borrow)]
        name: Cow<'a, str>,
        /// How long it took, in milliseconds.
        ms: i64,
    },
    /// A long-running tool is still working, so a reader's spinner survives it.
    ToolCallProgress {
        /// Which tool.
        #[serde(borrow)]
        name: Cow<'a, str>,
        /// How long it has been running, in milliseconds.
        elapsed_ms: i64,
    },
}

/// `POST /v1/runners/me/leases/{lease_id}/activity` request — a batch of frames.
///
/// One frame per request today; the array shape lets a later change coalesce
/// without a wire change. The reply is `202` with no acknowledgement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityRequest<'a> {
    /// The frames being forwarded.
    #[serde(borrow)]
    pub frames: Vec<ActivityFrame<'a>>,
}
