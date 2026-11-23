//! The two live frames a setup run's draft stream carries: the answer as it
//! arrives (`message.delta`) and its compaction's progress
//! (`context.progress`). Both are cosmetic hints; the report's answer and the
//! saved checkpoint are the durable facts.

use af_core::id::Uuid7;
use af_wire::context::ContextProgress;
use serde::{Deserialize, Serialize};

/// The room a delta's fields other than its text take in one frame.
const DELTA_ENVELOPE_BYTES: usize = 4096;

/// Maximum private answer bytes in one preview frame: the draft frame less its
/// envelope, over the six bytes one character can escape to in JSON.
pub(crate) const DELTA_BYTES: usize = (afd_bus::DRAFT_FRAME_BYTES - DELTA_ENVELOPE_BYTES) / 6;
/// Private answer event name, shared by the publisher and HTTP adapter.
pub const DELTA_EVENT: &str = "message.delta";
/// Compaction progress event name, shared by the publisher and HTTP adapter.
/// Creator-private metadata; generated summary text never enters it.
pub const CONTEXT_EVENT: &str = "context.progress";

/// Only answer text is eligible for a private preview.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextKind {
    /// Provider answer, excluding reasoning and tool arguments.
    Answer,
}

/// Ephemeral frame; a missing sequence requires a durable read, never a resend.
#[derive(Clone, Serialize, Deserialize, garde::Validate)]
pub struct Delta {
    /// The authorized draft subject.
    #[garde(skip)]
    pub draft_id: Uuid7,
    /// Durable message position as a decimal integer.
    #[garde(length(min = 1, max = 19), custom(af_validate::ascii_digits))]
    pub sequence: String,
    /// The run whose answer this is: a reassigned turn starts a new stream.
    #[garde(skip)]
    pub stream_id: Uuid7,
    /// Monotonic within one stream.
    #[garde(range(min = 1))]
    pub stream_seq: u64,
    /// The first frame of a new stream.
    #[garde(skip)]
    pub stream_start: bool,
    /// Excludes reasoning and tool parameters.
    #[garde(skip)]
    pub text_kind: TextKind,
    /// Bounded Unicode answer text.
    #[garde(length(bytes, min = 1, max = DELTA_BYTES))]
    pub text: String,
}

impl std::fmt::Debug for Delta {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Delta")
            .field("draft_id", &self.draft_id)
            .field("sequence", &self.sequence)
            .field("stream_id", &self.stream_id)
            .field("stream_seq", &self.stream_seq)
            .field("stream_start", &self.stream_start)
            .field("text_kind", &self.text_kind)
            .field("text_bytes", &self.text.len())
            .finish()
    }
}

impl Drop for Delta {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.text);
    }
}

/// One bounded compaction status hint; reconnecting readers clear it and
/// reload message state.
#[derive(Debug, Clone, Deserialize, Serialize, garde::Validate)]
#[serde(deny_unknown_fields)]
pub struct ContextNotice {
    /// The authenticated draft subject.
    #[garde(skip)]
    pub draft_id: Uuid7,
    /// The currently running user message.
    #[garde(length(min = 1, max = 19), custom(af_validate::ascii_digits))]
    pub sequence: String,
    /// The run compacting, independent of its answer's stream.
    #[garde(skip)]
    pub stream_id: Uuid7,
    /// Counts and closed state only.
    #[garde(dive)]
    pub progress: ContextProgress,
}

#[cfg(test)]
mod tests {
    use af_core::id::Uuid7;

    use super::{Delta, TextKind};

    /// A preview frame reaches logs through `Debug`, which names its size and
    /// never its private text.
    #[test]
    fn a_delta_never_prints_its_text() -> crate::Result<()> {
        let id = Uuid7::parse("01920000-0000-7000-8000-000000000001")?;
        let delta = Delta {
            draft_id: id.clone(),
            sequence: "2".into(),
            stream_id: id,
            stream_seq: 1,
            stream_start: true,
            text_kind: TextKind::Answer,
            text: "a private answer".into(),
        };
        let printed = format!("{delta:?}");
        assert!(!printed.contains("a private answer"), "{printed}");
        assert!(printed.contains("text_bytes: 16"), "{printed}");
        Ok(())
    }
}
