//! Each finished call's full record: every argument and the whole output, cut
//! to the bounds `af_wire::tool_detail` declares, so "show all" in the thread
//! has more than the trace's edges.
//!
//! A record must also fit one post on its own, so an output that escapes into
//! more than `DETAIL_POST_MAX_BYTES` of JSON is cut further until it does.

use std::borrow::Cow;

use af_wire::tool_detail::{
    DETAIL_FIELD_MAX_BYTES, DETAIL_POST_MAX_BYTES, ToolCallRecord, ToolCallRecordsRequest,
};
use af_wire::tool_trace::encoded_len;
use serde_json::{Map, Value};

use af_secrets::Clean;

/// Room a post's envelope takes around its one record, measured rather than
/// guessed: the request the supervisor seals a lone record in, empty, at the
/// largest fencing token, so a record that fits here fits whichever lease
/// posts it.
fn post_envelope_bytes() -> usize {
    encoded_len(&ToolCallRecordsRequest {
        fencing_token: u64::MAX,
        calls: Vec::new(),
    })
}

/// Call `number`'s record, from its scrubbed arguments and output.
pub(crate) fn record(
    number: u64,
    arguments: Clean<Value>,
    output: &Clean<String>,
) -> ToolCallRecord<'static> {
    let (arguments, truncated_arguments) = match arguments.into_inner() {
        Value::Object(fields) if encoded_len(&fields) <= DETAIL_FIELD_MAX_BYTES => (fields, false),
        Value::Object(_) => (Map::new(), true),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Array(_) => {
            (Map::new(), false)
        }
    };
    let mut record = ToolCallRecord {
        call_number: number,
        arguments,
        truncated_arguments,
        output: Cow::Owned(String::new()),
        output_line_count: u64::try_from(output.lines().count()).unwrap_or(u64::MAX),
        truncated: false,
    };
    let room = DETAIL_POST_MAX_BYTES.saturating_sub(post_envelope_bytes());
    let mut keep = output.floor_char_boundary(DETAIL_FIELD_MAX_BYTES);
    loop {
        record.output = Cow::Owned(output.get(..keep).unwrap_or_default().to_owned());
        record.truncated = keep < output.len();
        if keep == 0 || encoded_len(&record) <= room {
            return record;
        }
        keep = output.floor_char_boundary(keep.div_euclid(2));
    }
}

#[cfg(test)]
mod tests;
