//! Count actual JSON without allocating a second copy of the host's usage records.

use std::num::NonZeroU64;

use af_core::id::Uuid7;
use af_wire::protocol::{MESSAGE_BYTES_MAX, Renewal, Up};

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
    let overhead = af_validate::serialized_len(&empty).map_err(crate::error::encode)?;
    let mut bytes = overhead;
    let mut current = RenewalBatch {
        ids: Vec::new(),
        leases: Vec::new(),
    };
    let mut batches = Vec::new();
    for (id, renewal) in entries {
        let entry_bytes = af_validate::serialized_len(&renewal).map_err(crate::error::encode)?;
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
