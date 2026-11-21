//! A conversation's compacted context: the content-free progress of a
//! summary, and the fenced verb a setup run reads and saves its draft's
//! context through.
//!
//! ```text
//!  setup run start ─► ContextLoad { fencing_token } ─► { scope, resource_version, checkpoint? }
//!  window compacted ─► ContextSave { fencing_token, resource_version, checkpoint }
//!                        ─► { resource_version }   or refused: a superseded run writes nothing
//! ```
//!
//! The checkpoint rides as the `af_context` document it is, opaque here: this
//! crate carries primitives, and each end validates the document with
//! `af_context` before it trusts it. Its notes are the person's private
//! conversation, so no `Debug` here prints them.
use std::fmt;

use garde::Validate;
use serde::{Deserialize, Serialize};

/// Content-free state of one conversation summary.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextState {
    /// One metered summary call is in progress.
    Summarizing,
    /// A validated checkpoint committed and preparation continues.
    Resuming,
    /// The operation stopped without a confirmed checkpoint.
    Interrupted,
}

/// Progress never contains notes, source messages or model-generated prose.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ContextProgress {
    /// Monotonic checkpoint generation within this conversation branch.
    #[garde(range(min = 1, max = i64::MAX.unsigned_abs()))]
    pub generation: u64,
    /// Only confirmed publication may report resuming.
    #[garde(skip)]
    pub state: ContextState,
}

/// A setup run's read of its draft's compacted context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ContextLoadRequest {
    /// The fence the run holds its draft's thread at; a run a reassignment
    /// superseded is refused.
    #[garde(range(max = i64::MAX.unsigned_abs()))]
    pub fencing_token: u64,
}

/// The draft's compacted context, as a setup run starts from it.
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextLoadResponse {
    /// The scope every checkpoint of this draft names. The daemon assigns
    /// it, so the run saves under the scope the daemon checks.
    pub scope: String,
    /// The generation the stored context holds; zero before the draft's
    /// first save. The run's first save expects it.
    pub resource_version: u64,
    /// The stored `af_context` checkpoint document; absent before the
    /// draft's first save.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint: Option<serde_json::Value>,
}

impl fmt::Debug for ContextLoadResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ContextLoadResponse")
            .field("resource_version", &self.resource_version)
            .field("checkpoint", &self.checkpoint.is_some())
            .finish_non_exhaustive()
    }
}

/// A setup run's save of its draft's compacted context: the output of the
/// window hook that compacted it.
#[derive(Clone, PartialEq, Eq, Deserialize, Serialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ContextSaveRequest {
    /// The fence the run holds its draft's thread at; a run a reassignment
    /// superseded writes nothing.
    #[garde(range(max = i64::MAX.unsigned_abs()))]
    pub fencing_token: u64,
    /// The generation the save expects the stored context to hold: the
    /// stored row moves only from it.
    #[garde(range(max = i64::MAX.unsigned_abs()))]
    pub resource_version: u64,
    /// The `af_context` checkpoint document, one generation past
    /// `resource_version`. The daemon validates it before it stores it.
    #[garde(skip)]
    pub checkpoint: serde_json::Value,
}

impl fmt::Debug for ContextSaveRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ContextSaveRequest")
            .field("fencing_token", &self.fencing_token)
            .field("resource_version", &self.resource_version)
            .finish_non_exhaustive()
    }
}

/// A save the daemon stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContextSaveResponse {
    /// The generation now stored, which the run's next save expects.
    pub resource_version: u64,
}
