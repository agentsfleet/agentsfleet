//! A tool call's full arguments and output, kept so an operator can open more
//! than the first and last lines the trace shows.
//!
//! The runner posts each finished call under its lease's fence; the daemon
//! narrows each record, holds an event to a byte budget, and keeps one row per
//! call. Both sides read the bounds below. Arguments and output are scrubbed
//! runner-side, so resolved secret bytes never cross this boundary.

use std::borrow::Cow;

use afd_validate::PathTable;
use garde::Validate;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_json::{Map, Value};

use crate::tool_trace::{encoded_len, fields_free_of_nul, free_of_nul};

/// The smallest call number: calls count from 1.
const CALL_NUMBER_MIN: u64 = 1;

/// The largest call number the stored column, a `bigint`, can hold.
const CALL_NUMBER_MAX: u64 = i64::MAX.unsigned_abs();

/// The path garde reports the record-wide NUL rule under: the record itself.
const PATH_RECORD: &str = "";
const PATH_CALL_NUMBER: &str = "call_number";
const PATH_ARGUMENTS: &str = "arguments";
const PATH_OUTPUT: &str = "output";

/// The reason each broken bound is skipped under, malformed before too large
/// as the checks read before they moved onto the type.
const REASONS: PathTable<DetailRejection> = PathTable::new(
    &[
        (PATH_CALL_NUMBER, DetailRejection::Malformed),
        (PATH_RECORD, DetailRejection::Malformed),
        (PATH_ARGUMENTS, DetailRejection::TooLarge),
        (PATH_OUTPUT, DetailRejection::TooLarge),
    ],
    DetailRejection::Malformed,
);

/// What the record-wide rule reports; the reason comes from [`REASONS`].
const HOLDS_NUL: &str = "a string holds a NUL character";

/// What the arguments rule reports; the reason comes from [`REASONS`].
const ARGUMENTS_TOO_LARGE: &str = "the arguments encode past their bound";

/// The most bytes one record's output, or its encoded arguments, may hold.
pub const DETAIL_FIELD_MAX_BYTES: usize = 64 * 1024;

/// The most record bytes one event may keep, summed over its calls.
pub const DETAIL_EVENT_MAX_BYTES: usize = 1024 * 1024;

/// The largest body one post may send.
pub const DETAIL_POST_MAX_BYTES: usize = 256 * 1024;

/// The least a kept record spends of its event's budget, however small it is.
///
/// Each row costs storage beyond its payload — an id, its keys, three index
/// entries — so an event cannot keep hundreds of thousands of tiny records
/// inside a byte budget that counted only their payload.
pub const DETAIL_RECORD_MIN_BYTES: usize = 256;

/// One finished call as the runner records it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
// NUL is one rule over every string the record carries: Postgres refuses it in
// `text` and in `jsonb` alike.
#[garde(custom(record_free_of_nul))]
pub struct ToolCallRecord<'a> {
    /// The call's number in its run, from 1: the `n` of the `{fence}:{n}`
    /// call id the live frames and the trace carry.
    #[cfg_attr(
        feature = "openapi",
        schema(minimum = 1, maximum = 9_223_372_036_854_775_807_u64)
    )]
    #[garde(range(min = CALL_NUMBER_MIN, max = CALL_NUMBER_MAX))]
    pub call_number: u64,
    /// Every argument the call was made with, secret values masked. At most
    /// 65536 bytes encoded, with no NUL character.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    #[garde(custom(arguments_fit))]
    pub arguments: Map<String, Value>,
    /// Whether the runner cut the arguments to fit.
    #[garde(skip)]
    pub truncated_arguments: bool,
    /// Everything the call returned, at most 65536 bytes, with no NUL
    /// character.
    #[serde(borrow)]
    #[garde(length(bytes, max = DETAIL_FIELD_MAX_BYTES))]
    pub output: Cow<'a, str>,
    /// How many lines the whole output had, before any cut.
    #[garde(skip)]
    pub output_line_count: u64,
    /// Whether the runner cut the output to fit.
    #[garde(skip)]
    pub truncated: bool,
}

/// Every string a record carries, arguments included, is free of NUL.
fn record_free_of_nul<C: ?Sized>(record: &ToolCallRecord<'_>, _context: &C) -> garde::Result {
    if free_of_nul(&record.output) && fields_free_of_nul(&record.arguments) {
        Ok(())
    } else {
        Err(garde::Error::new(HOLDS_NUL))
    }
}

/// The arguments, encoded, within [`DETAIL_FIELD_MAX_BYTES`].
fn arguments_fit<C: ?Sized>(arguments: &Map<String, Value>, _context: &C) -> garde::Result {
    if encoded_len(arguments) <= DETAIL_FIELD_MAX_BYTES {
        Ok(())
    } else {
        Err(garde::Error::new(ARGUMENTS_TOO_LARGE))
    }
}

impl ToolCallRecord<'_> {
    /// The bytes this record spends of its event's budget: its arguments
    /// encoded, plus its output, and never less than
    /// [`DETAIL_RECORD_MIN_BYTES`].
    #[must_use]
    pub fn byte_count(&self) -> usize {
        encoded_len(&self.arguments)
            .saturating_add(self.output.len())
            .max(DETAIL_RECORD_MIN_BYTES)
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
    /// Arguments or output over [`DETAIL_FIELD_MAX_BYTES`].
    TooLarge,
    /// Keeping it would carry the event past [`DETAIL_EVENT_MAX_BYTES`].
    OverBudget,
}

impl DetailRejection {
    /// The reason as a skip log spells it.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Malformed => "malformed",
            Self::TooLarge => "too_large",
            Self::OverBudget => "over_budget",
        }
    }
}

/// `POST /v1/runners/me/leases/{lease_id}/tool-calls` request.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCallRecordsRequest<'a> {
    /// The lease's fencing token; a holder the fleet has superseded is refused.
    pub fencing_token: u64,
    /// The finished calls to keep. A call posted again replaces its record.
    #[serde(borrow)]
    #[cfg_attr(feature = "openapi", schema(value_type = Vec<ToolCallRecord>))]
    pub calls: Vec<RawToolCallRecord<'a>>,
}

/// What a post kept.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCallRecordsStored {
    /// Records written or replaced.
    pub stored_count: usize,
    /// Records refused for their shape, a bound, or the event's budget.
    pub skipped_count: usize,
}

/// `GET …/events/{event_id}/tool-calls/{call_id}` reply: one call, in full.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCallDetail<'a> {
    /// The call, as the thread names it: `{fence}:{n}`.
    #[serde(borrow)]
    pub call_id: Cow<'a, str>,
    /// Every argument the call was made with, secret values masked.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub arguments: Map<String, Value>,
    /// Whether the arguments were cut to fit 65536 bytes.
    pub truncated_arguments: bool,
    /// Everything the call returned.
    #[serde(borrow)]
    pub output: Cow<'a, str>,
    /// How many lines the whole output had, before any cut.
    pub output_line_count: u64,
    /// Whether the output was cut to fit 65536 bytes.
    pub truncated: bool,
}

#[cfg(test)]
#[path = "tool_detail/tests.rs"]
mod tests;
