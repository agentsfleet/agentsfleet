//! A bounded preview; durable receipts remain the only source of final answers.

mod publish;
#[cfg(test)]
mod tests;
mod trace;

use afd_core::id::Uuid7;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use zeroize::Zeroizing;

pub(super) use publish::Publisher;
pub(super) const CAPACITY: usize = 32;
pub(super) const FLUSH: std::time::Duration = std::time::Duration::from_millis(50);
/// Maximum private answer bytes in one preview frame.
pub(super) const DELTA_BYTES: usize = 4096;
/// Private answer event name, shared by the publisher and HTTP adapter.
pub const DELTA_EVENT: &str = "message.delta";

/// The subject is private; no workspace or fleet activity channel carries it.
#[must_use]
pub fn draft_channel(draft: &Uuid7) -> String {
    format!("draft:{draft}:events")
}

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
    #[garde(length(min = 1, max = 19), custom(afd_validate::ascii_digits))]
    pub sequence: String,
    /// Unique physical provider dispatch, including subsequent tool passes.
    #[garde(skip)]
    pub stream_id: Uuid7,
    /// Monotonic within one dispatch; assigned before a bounded queue send.
    #[garde(range(min = 1))]
    pub stream_seq: u64,
    /// The first frame of a new physical dispatch.
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

pub(super) struct Preview {
    sender: mpsc::Sender<Piece>,
    buffer: Zeroizing<String>,
    sequence: u64,
}

impl Drop for Delta {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.text);
    }
}

pub(super) struct Piece {
    pub(super) sequence: u64,
    pub(super) text: Zeroizing<String>,
}

impl Preview {
    pub(super) fn new(sender: mpsc::Sender<Piece>) -> Self {
        Self {
            sender,
            buffer: Zeroizing::new(String::with_capacity(DELTA_BYTES)),
            sequence: 0,
        }
    }

    pub(super) fn push(&mut self, text: &str) {
        // Fragment at character boundaries without building another full answer.
        for character in text.chars() {
            if self.buffer.len() + character.len_utf8() > DELTA_BYTES {
                self.flush();
            }
            self.buffer.push(character);
        }
        if self.sequence == 0 {
            self.flush();
        }
    }

    pub(super) fn flush(&mut self) {
        if self.buffer.is_empty() {
            return;
        }
        self.sequence += 1;
        let text = std::mem::replace(
            &mut self.buffer,
            Zeroizing::new(String::with_capacity(DELTA_BYTES)),
        );
        // A saturated or revoked preview cannot delay accounting or inference.
        // Its assigned sequence exposes the gap to a reader that later catches up.
        let _queued = self.sender.try_send(Piece {
            sequence: self.sequence,
            text,
        });
    }
}
