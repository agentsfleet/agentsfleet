//! Which of a push's deltas a store may write, and the tallies for the rest.
//!
//! Decided here, before any store is reached, so every store enforces the same
//! bounds and the same publish grant without restating either.

use afd_wire::memory::{MAX_PUSH_BYTES, MemoryDelta};
use garde::Validate as _;

/// The deltas a push may store, and the tallies for those it may not.
#[derive(Debug)]
pub(crate) struct Admitted<'a, 'b> {
    /// The deltas a store writes, in push order.
    pub(crate) entries: Vec<&'a MemoryDelta<'b>>,
    /// Refused for shape: an empty or oversized field.
    pub(crate) skipped: usize,
    /// Well-formed, but past the push byte cap, which ends the batch.
    pub(crate) truncated: usize,
    /// Workspace-visible, from a fleet without the publish grant.
    pub(crate) unpublished: usize,
}

/// Filter a push to the deltas worth storing.
///
/// Three refusals, counted apart because they mean different things. A
/// SKIPPED delta is malformed and the rest of the batch is unaffected. An
/// UNPUBLISHED delta asks the workspace to read it and the fleet may not
/// publish — the grant holds even against a runner that skipped the tool's own
/// check. A TRUNCATED delta is well-formed and past the push byte cap, which
/// ENDS the batch: the cap bounds one request, so everything after the entry
/// that crosses it is refused too.
pub(crate) fn admit<'a, 'b>(deltas: &'a [MemoryDelta<'b>], publishes: bool) -> Admitted<'a, 'b> {
    let mut entries: Vec<_> = deltas
        .iter()
        .filter(|delta| delta.validate().is_ok())
        .collect();
    let skipped = deltas.len() - entries.len();
    let shaped = entries.len();
    entries.retain(|delta| publishes || !delta.visibility.is_workspace());
    let unpublished = shaped - entries.len();

    // A running total over the deltas still standing, so a refused one neither
    // consumes budget nor ends the batch.
    let fits = entries
        .iter()
        .scan(0_usize, |used, delta| {
            *used += delta.bytes();
            Some(*used)
        })
        .take_while(|used| *used <= MAX_PUSH_BYTES)
        .count();
    let truncated = entries.len() - fits;
    entries.truncate(fits);
    Admitted {
        entries,
        skipped,
        truncated,
        unpublished,
    }
}

#[cfg(test)]
#[path = "admit_tests.rs"]
mod tests;
