//! A run's full tool-call records, packed into the posts the daemon takes.
//!
//! Each body carries the lease's fencing token and as many records as fit in
//! `DETAIL_POST_MAX_BYTES`; the agent already cut every record to fit a post
//! on its own. Records are encoded once and carried as raw JSON.

use afd_wire::tool_detail::{
    DETAIL_POST_MAX_BYTES, RawToolCallRecord, ToolCallRecord, ToolCallRecordsRequest,
};
use bytes::Bytes;

use crate::error::{self, Result};

/// The bodies that post `records` under `fencing_token`, in order.
///
/// # Errors
/// A record that would not encode.
pub(crate) fn bodies(fencing_token: u64, records: &[ToolCallRecord<'_>]) -> Result<Vec<Bytes>> {
    let encoded = records
        .iter()
        .map(serde_json::to_string)
        .collect::<Result<Vec<String>, _>>()
        .map_err(error::encode)?;
    let envelope = seal(fencing_token, Vec::new())?.len();
    let mut bodies = Vec::new();
    let mut batch: Vec<RawToolCallRecord<'_>> = Vec::new();
    let mut size = envelope;
    for text in &encoded {
        // A comma joins each record after a body's first.
        if !batch.is_empty() && size + 1 + text.len() > DETAIL_POST_MAX_BYTES {
            bodies.push(seal(fencing_token, std::mem::take(&mut batch))?);
            size = envelope;
        }
        size += usize::from(!batch.is_empty()) + text.len();
        batch.push(serde_json::from_str(text).map_err(error::encode)?);
    }
    if !batch.is_empty() {
        bodies.push(seal(fencing_token, batch)?);
    }
    Ok(bodies)
}

/// One post's body.
fn seal(fencing_token: u64, calls: Vec<RawToolCallRecord<'_>>) -> Result<Bytes> {
    let request = ToolCallRecordsRequest {
        fencing_token,
        calls,
    };
    serde_json::to_vec(&request)
        .map(Bytes::from)
        .map_err(error::encode)
}

#[cfg(test)]
#[path = "records/tests.rs"]
mod tests;
