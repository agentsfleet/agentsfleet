//! Count actual JSON without allocating a second copy of the host's usage records.

use std::{io, num::NonZeroU64};

use af_core::id::Uuid7;
use af_wire::protocol::{MESSAGE_BYTES_MAX, Renewal, Up};
use serde::Serialize;

use crate::{Result, client::stream::RenewalBatch, error::ErrorKind};

/// Stable order and a maximal request identifier make every partition safe for any session id.
pub(super) fn split(
    mut entries: Vec<(Uuid7, Renewal<'static>)>,
) -> Result<Vec<RenewalBatch<'static>>> {
    entries.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    let empty: Up<'_> = Up::RenewBatch {
        request_id: NonZeroU64::MAX,
        leases: Vec::new(),
    };
    let overhead = encoded_bytes(&empty)?;
    let mut bytes = overhead;
    let mut current = RenewalBatch {
        ids: Vec::new(),
        leases: Vec::new(),
    };
    let mut batches = Vec::new();
    for (id, renewal) in entries {
        let entry_bytes = encoded_bytes(&renewal)?;
        if overhead + entry_bytes > MESSAGE_BYTES_MAX {
            return Err(ErrorKind::StreamProtocol.into());
        }
        let separator = usize::from(!current.leases.is_empty());
        if bytes + separator + entry_bytes > MESSAGE_BYTES_MAX {
            batches.push(current);
            current = RenewalBatch {
                ids: Vec::new(),
                leases: Vec::new(),
            };
            bytes = overhead;
        }
        bytes += usize::from(!current.leases.is_empty()) + entry_bytes;
        current.ids.push(id);
        current.leases.push(renewal);
    }
    if !current.leases.is_empty() {
        batches.push(current);
    }
    Ok(batches)
}

fn encoded_bytes(value: &impl Serialize) -> Result<usize> {
    let mut counter = Counter(0);
    serde_json::to_writer(&mut counter, value).map_err(crate::error::encode)?;
    Ok(counter.0)
}

/// Mirrors activity's counting writer; no encoded request allocation survives sizing.
struct Counter(usize);

impl io::Write for Counter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0 = self.0.saturating_add(buf.len());
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
