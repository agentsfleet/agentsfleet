//! The `custom` rules a trace is declared with.
//!
//! Each rule reports the spelling of the [`TraceRejection`] it is, so a garde
//! report maps back to the reason a drop logs (`TraceRejection::of`). The one
//! bound garde writes itself, the call count, is named by its path instead.

use serde_json::{Map, Value};

use crate::activity::CALL_ID_MAX_BYTES;

use super::{
    ARGS_LEAF_MAX_BYTES, ARGS_MAX_BYTES, TRACE_MAX_BYTES, ToolTrace, ToolTraceCall, TraceRejection,
    edge_fits, encoded_len, fields_free_of_nul, free_of_nul,
};

/// The smallest call number: calls count from 1.
const FIRST_CALL: i64 = 1;

/// `Ok` when `holds`, otherwise a report naming `rejection`.
fn refuse_unless(holds: bool, rejection: TraceRejection) -> garde::Result {
    if holds {
        Ok(())
    } else {
        Err(garde::Error::new(rejection.as_str()))
    }
}

/// The whole trace, encoded, within [`TRACE_MAX_BYTES`].
pub(super) fn trace_fits<C: ?Sized>(trace: &ToolTrace<'_>, _context: &C) -> garde::Result {
    refuse_unless(
        encoded_len(trace) <= TRACE_MAX_BYTES,
        TraceRejection::TooLarge,
    )
}

/// Every string one call carries, arguments included, free of NUL.
pub(super) fn call_free_of_nul<C: ?Sized>(call: &ToolTraceCall<'_>, _context: &C) -> garde::Result {
    let edges = [call.output_head.as_deref(), call.output_tail.as_deref()];
    let clean = free_of_nul(&call.call_id)
        && free_of_nul(&call.name)
        && edges.into_iter().flatten().all(free_of_nul)
        && fields_free_of_nul(&call.arguments);
    refuse_unless(clean, TraceRejection::HoldsNul)
}

/// A call id is a call number the record verb keys by: decimal digits naming
/// 1 to `i64::MAX`, within the frame's [`CALL_ID_MAX_BYTES`].
///
/// The trace's id is what "show all" resolves, as `{fence}:{call_id}`, so an
/// id the read cannot parse would be a call whose full output can never be
/// opened. The byte bound is not implied by the number's: leading zeros parse,
/// so `0…01` names call 1 at any length.
pub(super) fn call_number<C: ?Sized>(call_id: &str, context: &C) -> garde::Result {
    let numbered = call_id.len() <= CALL_ID_MAX_BYTES
        && afd_validate::ascii_digits(call_id, context).is_ok()
        && call_id
            .parse::<i64>()
            .is_ok_and(|number| number >= FIRST_CALL);
    refuse_unless(numbered, TraceRejection::CallIdUnusable)
}

/// The arguments object, encoded, within [`ARGS_MAX_BYTES`].
pub(super) fn arguments_fit<C: ?Sized>(
    arguments: &Map<String, Value>,
    _context: &C,
) -> garde::Result {
    refuse_unless(
        encoded_len(arguments) <= ARGS_MAX_BYTES,
        TraceRejection::ArgumentsTooLarge,
    )
}

/// Every key and string inside the arguments within [`ARGS_LEAF_MAX_BYTES`].
pub(super) fn argument_leaves_fit<C: ?Sized>(
    arguments: &Map<String, Value>,
    _context: &C,
) -> garde::Result {
    refuse_unless(fields_fit(arguments), TraceRejection::ArgumentTooLong)
}

/// One output edge within its byte and line bounds.
pub(super) fn edge<C: ?Sized>(edge: &str, _context: &C) -> garde::Result {
    refuse_unless(edge_fits(edge), TraceRejection::EdgeTooLarge)
}

/// Whether every key and string inside `fields` is within
/// [`ARGS_LEAF_MAX_BYTES`].
fn fields_fit(fields: &Map<String, Value>) -> bool {
    fields
        .iter()
        .all(|(key, value)| key.len() <= ARGS_LEAF_MAX_BYTES && leaves_fit(value))
}

/// Whether every key and string inside `value` is within
/// [`ARGS_LEAF_MAX_BYTES`].
///
/// Recursion is bounded by the parser, which refuses documents nested past its
/// own depth limit before a value reaches here.
fn leaves_fit(value: &Value) -> bool {
    match value {
        Value::String(text) => text.len() <= ARGS_LEAF_MAX_BYTES,
        Value::Array(items) => items.iter().all(leaves_fit),
        Value::Object(fields) => fields_fit(fields),
        Value::Null | Value::Bool(_) | Value::Number(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::{CALL_ID_MAX_BYTES, call_number};

    /// Call 1, zero-padded to `bytes`.
    fn padded(bytes: usize) -> String {
        format!("{}1", "0".repeat(bytes - 1))
    }

    /// Leading zeros parse, so the byte bound is a check of its own: call 1
    /// spelled at the bound is a call id, and one byte past it is not.
    #[test]
    fn a_call_id_is_held_to_the_frame_bound_however_it_is_padded() {
        assert_eq!(call_number(&padded(CALL_ID_MAX_BYTES), &()), Ok(()));
        assert!(call_number(&padded(CALL_ID_MAX_BYTES + 1), &()).is_err());
    }
}
