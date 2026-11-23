//! A tool call's full arguments and output, kept so an operator can open more
//! than the first and last lines the trace shows.
//!
//! The runner posts each finished call under its run's fence; the daemon
//! narrows each record and keeps one row per call. No field, event or post
//! cap bounds a record: a post is bounded by the runner frame
//! (`crate::protocol::MESSAGE_BYTES_MAX`), and the runner cuts only a record
//! that would not fit one on its own. Arguments and output are scrubbed
//! runner-side, so resolved secret bytes never cross this boundary.

use std::borrow::Cow;

use af_validate::PathTable;
use garde::Validate;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_json::{Map, Value};

use crate::tool_trace::{FileChange, changes_free_of_nul, fields_free_of_nul, free_of_nul};

mod item;
pub use self::item::{ItemKind, ItemStatus};

/// The smallest call number: calls count from 1.
const CALL_NUMBER_MIN: u64 = 1;

/// The largest call number the stored column, a `bigint`, can hold.
const CALL_NUMBER_MAX: u64 = i64::MAX.unsigned_abs();

/// A turn id's length in bytes: a version 7 UUID in its canonical text form.
const TURN_ID_BYTES: usize = 36;

/// The path garde reports the record-wide NUL rule under: the record itself.
const PATH_RECORD: &str = "";
const PATH_CALL_NUMBER: &str = "call_number";

/// The reason each broken bound is skipped under.
const REASONS: PathTable<DetailRejection> = PathTable::new(
    &[
        (PATH_CALL_NUMBER, DetailRejection::Malformed),
        (PATH_RECORD, DetailRejection::Malformed),
    ],
    DetailRejection::Malformed,
);

/// What the record-wide rule reports; the reason comes from [`REASONS`].
const HOLDS_NUL: &str = "a string holds a NUL character";

/// One finished call as the runner records it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
// NUL is one rule over every string the record carries: Postgres refuses it in
// `text` and in `jsonb` alike.
#[garde(custom(record_free_of_nul))]
pub struct ToolCallRecord<'a> {
    /// Call number within its run, starting at 1. This is `n` in the `{fence}:{n}` identifier
    /// used by activity and saved records.
    #[cfg_attr(
        feature = "openapi",
        schema(minimum = 1, maximum = 9_223_372_036_854_775_807_u64)
    )]
    #[garde(range(min = CALL_NUMBER_MIN, max = CALL_NUMBER_MAX))]
    pub call_number: u64,
    /// Every argument the call was made with, secret values masked, with no
    /// NUL character.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    #[garde(skip)]
    pub arguments: Map<String, Value>,
    /// Whether the runner dropped the arguments, which alone would not fit
    /// one post.
    #[garde(skip)]
    pub truncated_arguments: bool,
    /// Everything the call returned, with no NUL character. Cut only when
    /// the record would not otherwise fit one post.
    #[serde(borrow)]
    #[garde(skip)]
    pub output: Cow<'a, str>,
    /// How many lines the whole output had, before any cut.
    #[garde(skip)]
    pub output_line_count: u64,
    /// Whether the runner cut the output to fit one post.
    #[garde(skip)]
    pub truncated: bool,
    /// The turn that was newest when the call's step began: the run's own,
    /// or one a person sent while it worked and its conversation read. A turn
    /// the run does not serve is refused.
    #[serde(borrow)]
    #[garde(
        length(bytes, min = TURN_ID_BYTES, max = TURN_ID_BYTES),
        pattern(r"^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
    )]
    pub turn_id: Cow<'a, str>,
    /// What kind of thing the call was.
    #[garde(skip)]
    pub kind: ItemKind,
    /// The tool it called; never empty.
    #[serde(borrow)]
    #[garde(length(bytes, min = 1))]
    pub tool: Cow<'a, str>,
    /// How it ended.
    #[garde(skip)]
    pub status: ItemStatus,
    /// For an edit: each file it changed and the lines it added and removed.
    /// Absent for any other call.
    #[serde(borrow, default, skip_serializing_if = "Vec::is_empty")]
    #[garde(dive)]
    pub changes: Vec<FileChange<'a>>,
}

/// Every string a record carries, arguments included, is free of NUL.
fn record_free_of_nul<C: ?Sized>(record: &ToolCallRecord<'_>, _context: &C) -> garde::Result {
    let clean = free_of_nul(&record.output)
        && free_of_nul(&record.tool)
        && fields_free_of_nul(&record.arguments)
        && changes_free_of_nul(&record.changes);
    if clean {
        Ok(())
    } else {
        Err(garde::Error::new(HOLDS_NUL))
    }
}

/// One record as a post carries it, before anything has read it.
///
/// Read one at a time, so a record of the wrong shape is skipped and counted
/// while the rest of the post is kept.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RawToolCallRecord<'a>(#[serde(borrow)] &'a RawValue);

impl PartialEq for RawToolCallRecord<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.0.get() == other.0.get()
    }
}

impl Eq for RawToolCallRecord<'_> {}

impl<'a> RawToolCallRecord<'a> {
    /// How many bytes the runner sent for this record.
    #[must_use]
    pub fn byte_len(self) -> usize {
        self.0.get().len()
    }

    /// The record these bytes hold, if it is one and fits its bounds.
    ///
    /// # Errors
    /// The [`DetailRejection`] naming why the record cannot be kept.
    pub fn narrow(self) -> Result<ToolCallRecord<'a>, DetailRejection> {
        let record: ToolCallRecord<'a> =
            serde_json::from_str(self.0.get()).map_err(|_shape| DetailRejection::Malformed)?;
        garde::Unvalidated::new(record)
            .validate()
            .map(garde::Valid::into_inner)
            .map_err(|report| REASONS.pick(&report))
    }
}

/// Why one record was not kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailRejection {
    /// Not a record: the wrong shape, a call number outside 1 to `i64::MAX`,
    /// or a NUL character the store cannot hold.
    Malformed,
    /// It names a turn the run does not serve: neither the run's own nor one
    /// its conversation read.
    UnservedTurn,
}

impl DetailRejection {
    /// The reason as a skip log spells it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Malformed => "malformed",
            Self::UnservedTurn => "unserved_turn",
        }
    }
}

/// `POST /v1/runners/me/runs/{run_id}/items` request.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCallRecordsRequest<'a> {
    /// The run's fencing token; a holder the fleet has superseded is refused.
    pub fencing_token: u64,
    /// The finished calls to keep. A call posted again replaces its record.
    #[serde(borrow)]
    #[cfg_attr(feature = "openapi", schema(value_type = Vec<ToolCallRecord<'static>>))]
    pub calls: Vec<RawToolCallRecord<'a>>,
}

/// What a post kept.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallRecordsStored {
    /// Records written or replaced.
    pub stored_count: usize,
    /// Records refused for their shape or for naming a turn the run does not
    /// serve.
    pub skipped_count: usize,
}

/// `GET …/turns/{turn_id}/items/{item_id}` reply: one call, in full.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCallDetail<'a> {
    /// Tool call identifier from the conversation, in `{fence}:{n}` format.
    #[serde(borrow)]
    pub call_id: Cow<'a, str>,
    /// What kind of thing the call was.
    pub kind: ItemKind,
    /// The tool it called. Absent for an item that called none.
    #[serde(borrow, default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<Cow<'a, str>>,
    /// How it ended.
    pub status: ItemStatus,
    /// Every argument the call was made with, secret values masked.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub arguments: Map<String, Value>,
    /// Whether the arguments were dropped because they alone would not fit
    /// one post.
    pub truncated_arguments: bool,
    /// Everything the call returned.
    #[serde(borrow)]
    pub output: Cow<'a, str>,
    /// How many lines the whole output had, before any cut.
    pub output_line_count: u64,
    /// Whether the output was cut to fit one post.
    pub truncated: bool,
    /// For an edit: each file it changed and the lines it added and
    /// removed. Absent for any other call.
    #[serde(borrow, default, skip_serializing_if = "Vec::is_empty")]
    pub changes: Vec<FileChange<'a>>,
}

#[cfg(test)]
mod tests;
