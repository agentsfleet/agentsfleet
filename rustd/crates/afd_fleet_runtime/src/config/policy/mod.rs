//! The three value objects a run is bounded by: what it may spend, where it
//! may reach, and how much context it may assemble.
//!
//! Each is built from its deserialized schema in one `TryFrom`, so the type
//! that leaves this module is already checked and nothing downstream re-checks
//! it.

use crate::config::raw;
use crate::error::{Error, ErrorKind, Result};

/// Where a fleet may reach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Network {
    /// Hosts it may reach at all.
    allow: Box<[Box<str>]>,
    /// Whether egress is read-only.
    read_only: bool,
    /// Paths that stay writable even under [`read_only`](Self::read_only).
    read_post_paths: Box<[Box<str>]>,
}

impl Network {
    /// Hosts this fleet may reach.
    #[must_use]
    pub fn allow(&self) -> &[Box<str>] {
        &self.allow
    }

    /// Whether egress is read-only.
    #[must_use]
    pub const fn read_only(&self) -> bool {
        self.read_only
    }

    /// Paths that stay writable under [`read_only`](Self::read_only).
    #[must_use]
    pub fn read_post_paths(&self) -> &[Box<str>] {
        &self.read_post_paths
    }
}

impl TryFrom<raw::Network> for Network {
    type Error = Error;

    fn try_from(authored: raw::Network) -> Result<Self> {
        // No bounds here: the schema declared them and garde already proved
        // them. What is left is ownership, which is a `map`.
        let entries = |items: Option<Vec<String>>| {
            items
                .unwrap_or_default()
                .into_iter()
                .map(Into::into)
                .collect()
        };

        Ok(Self {
            allow: entries(authored.allow),
            // Absent reads as false: the permissive value is the one an author
            // gets by not thinking about it, and it is what every existing
            // document was written against.
            read_only: authored.read_only.unwrap_or(false),
            read_post_paths: entries(authored.read_post_paths),
        })
    }
}

/// How much context a run may assemble.
///
/// Zero means "auto" throughout — the runner substitutes its own default. That
/// sentinel is the runner's existing contract rather than something introduced
/// here, which is why [`raw::Knob`] resolves the word `"auto"` to it at the
/// boundary instead of carrying an `Option` the runner would have to re-read.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ContextBudget {
    /// Ceiling on the assembled context, in tokens.
    pub context_cap_tokens: u32,
    /// How much of the window tool output may occupy.
    pub tool_window: u32,
    /// How often the run checkpoints its memory.
    pub memory_checkpoint_every: u32,
    /// The fraction of the window that triggers stage chunking.
    pub stage_chunk_threshold: f32,
}

impl TryFrom<raw::Context> for ContextBudget {
    type Error = Error;

    fn try_from(authored: raw::Context) -> Result<Self> {
        if let Some(unknown) = authored.extra.keys().next() {
            return Err(ErrorKind::UnknownRuntimeKey {
                field: unknown.as_str().into(),
            }
            .into());
        }

        Ok(Self {
            context_cap_tokens: authored.context_cap_tokens.map_or(0, raw::Knob::or_auto),
            tool_window: authored.tool_window.map_or(0, raw::Knob::or_auto),
            memory_checkpoint_every: authored
                .memory_checkpoint_every
                .map_or(0, raw::Knob::or_auto),
            stage_chunk_threshold: authored.stage_chunk_threshold.unwrap_or(0.0),
        })
    }
}

#[cfg(test)]
mod tests;
