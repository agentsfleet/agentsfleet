//! A tool call's full arguments and output, kept so an operator can open more
//! than the first and last lines the trace shows.
//!
//! The runner posts each finished call under its lease's fence; the daemon
//! narrows each record, holds an event to a byte budget, and keeps one row per
//! call. Both sides read the bounds below. Arguments and output are scrubbed
//! runner-side, so resolved secret bytes never cross this boundary.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_json::{Map, Value};

/// The most bytes one record's output, or its encoded arguments, may hold.
pub const DETAIL_FIELD_MAX_BYTES: usize = 64 * 1024;

/// The most record bytes one event may keep, summed over its calls.
pub const DETAIL_EVENT_MAX_BYTES: usize = 1024 * 1024;

/// The largest body one post may send.
pub const DETAIL_POST_MAX_BYTES: usize = 256 * 1024;

/// One finished call as the runner records it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ToolCallRecord<'a> {
    /// The call's number in its run, from 1: the `n` of the `{fence}:{n}`
    /// call id the live frames and the trace carry.
    pub call_number: u64,
    /// Every argument the call was made with, secret values masked. At most
    /// 65536 bytes encoded.
    #[cfg_attr(feature = "openapi", schema(value_type = Object))]
    pub arguments: Map<String, Value>,
    /// Whether the runner cut the arguments to fit.
    pub truncated_arguments: bool,
    /// Everything the call returned, at most 65536 bytes.
    #[serde(borrow)]
    pub output: Cow<'a, str>,
    /// How many lines the whole output had, before any cut.
    pub output_line_count: u64,
    /// Whether the runner cut the output to fit.
    pub truncated: bool,
}

impl ToolCallRecord<'_> {
    /// The bytes this record spends of its event's budget: its arguments
    /// encoded, plus its output.
    #[must_use]
    pub fn byte_count(&self) -> usize {
        arguments_len(&self.arguments).saturating_add(self.output.len())
    }

    /// Check this record's own bounds.
    ///
    /// # Errors
    /// The [`DetailRejection`] naming the bound it breaks.
    pub fn validate(&self) -> Result<(), DetailRejection> {
        let numbered = self.call_number >= 1 && i64::try_from(self.call_number).is_ok();
        if !numbered {
            return Err(DetailRejection::Malformed);
        }
        if arguments_len(&self.arguments) > DETAIL_FIELD_MAX_BYTES
            || self.output.len() > DETAIL_FIELD_MAX_BYTES
        {
            return Err(DetailRejection::TooLarge);
        }
        Ok(())
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
        record.validate()?;
        Ok(record)
    }
}

/// Why one record was not kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DetailRejection {
    /// Not a record: the wrong shape, or a call number outside 1 to
    /// `i64::MAX`.
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
#[serde(deny_unknown_fields)]
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

/// How many bytes `arguments` encodes to as compact JSON.
///
/// A map of string keys and JSON values always encodes; a failure would
/// answer the largest size, which every bound refuses.
fn arguments_len(arguments: &Map<String, Value>) -> usize {
    serde_json::to_vec(arguments).map_or(usize::MAX, |bytes| bytes.len())
}

#[cfg(test)]
#[path = "tool_detail/tests.rs"]
mod tests;
