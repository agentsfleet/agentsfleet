//! Each finished call's full record: every argument and the whole output, so
//! "show all" in the thread has more than the trace's edges.
//!
//! No field cap applies. A record must still fit one post on its own, and a
//! post is one runner frame (`MESSAGE_BYTES_MAX`), so arguments that alone
//! would not fit are dropped, and an output that encodes past it is halved
//! until the record fits.

use std::borrow::Cow;

use af_wire::protocol::MESSAGE_BYTES_MAX;
use af_wire::tool_detail::{ItemKind, ItemStatus, ToolCallRecord, ToolCallRecordsRequest};
use af_wire::tool_trace::{FileChange, encoded_len};
use serde_json::{Map, Value};

use af_secrets::Clean;

/// Room a post's envelope takes around its one record, measured rather than
/// guessed: the request the supervisor seals a lone record in, empty, at the
/// largest fencing token, so a record that fits here fits whichever run
/// posts it.
fn post_envelope_bytes() -> usize {
    encoded_len(&ToolCallRecordsRequest {
        fencing_token: u64::MAX,
        calls: Vec::new(),
    })
}

/// What a record says of its call besides the arguments and the output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Item {
    /// The turn newest when the call's step began.
    pub(crate) turn_id: String,
    /// What kind of thing the call was.
    pub(crate) kind: ItemKind,
    /// The tool it called.
    pub(crate) tool: String,
    /// How it ended.
    pub(crate) status: ItemStatus,
    /// For an edit, each file it changed, read from its scrubbed output.
    pub(crate) changes: Vec<FileChange<'static>>,
}

/// Call `number`'s record, from its scrubbed arguments and output, and what
/// `item` says of it.
pub(crate) fn record(
    number: u64,
    arguments: Clean<Value>,
    output: &Clean<String>,
    item: Item,
) -> ToolCallRecord<'static> {
    let room = MESSAGE_BYTES_MAX.saturating_sub(post_envelope_bytes());
    let (arguments, truncated_arguments) = match arguments.into_inner() {
        Value::Object(fields) if encoded_len(&fields) <= room => (fields, false),
        Value::Object(_) => (Map::new(), true),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Array(_) => {
            (Map::new(), false)
        }
    };
    let Item {
        turn_id,
        kind,
        tool,
        status,
        changes,
    } = item;
    let mut record = ToolCallRecord {
        call_number: number,
        arguments,
        truncated_arguments,
        output: Cow::Owned(String::new()),
        output_line_count: u64::try_from(output.lines().count()).unwrap_or(u64::MAX),
        truncated: false,
        turn_id: Cow::Owned(turn_id),
        kind,
        tool: Cow::Owned(tool),
        status,
        changes,
    };
    let mut keep = output.len();
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
