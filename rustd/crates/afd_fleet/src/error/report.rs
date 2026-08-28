//! The raisers that REPORT something went wrong.
//!
//! The mirror image of [`super::refuse`], split from [`super`] along the line
//! that module's own header draws. A refusal there says the system worked and
//! the answer is no; every constructor here says the system did not work — a
//! statement Postgres would not run, a column holding a value this daemon
//! cannot read, an object store that answered with a fault, a stream entry the
//! producer wrote wrong.
//!
//! Splitting them was the file cap's doing and it improved the module: what is
//! left in [`super`] is now the type, the alias and the kinds, so a reader
//! looking for what a failure IS no longer scrolls past ninety lines of how one
//! is built. Every name here is re-exported from [`super`], so no call site
//! spells this module.

use super::{Error, ErrorKind};

/// Reports a stream entry that does not satisfy the producer's contract.
///
/// `field` is `&'static str` rather than an owned name because every caller
/// passes one of the envelope's own constants — a name that had to be
/// allocated would mean it came from somewhere other than the contract.
pub(crate) fn envelope_field(field: &'static str) -> Error {
    Error::new(ErrorKind::Envelope { field })
}

/// Refuses a request the caller can correct, quoting the Zig detail verbatim.
pub(crate) fn rejected(detail: &'static str) -> Error {
    Error::new(ErrorKind::Rejected { detail })
}

/// Reports a stored credential body that is not an addressable object.
pub(crate) fn vault_data_invalid() -> Error {
    Error::new(ErrorKind::VaultDataInvalid)
}

/// Reports a stored fencing sequence that cannot be one.
///
/// Its own kind rather than a saturating read, and the direction is why.
/// [`Fence::as_u64`](crate::lease::Fence::as_u64) saturates a negative token to
/// ZERO because zero is below every token a claim can mint, so a corrupt row
/// fences ITSELF out. The live sequence a memory push is checked against runs
/// the other way: saturating it to zero would put it below every token in
/// existence and admit every stale holder. There is no safe value, so there is
/// no value.
pub(crate) fn sequence_corrupt() -> Error {
    Error::new(ErrorKind::SequenceCorrupt)
}

/// Reports a content hash with no snapshot stored under it.
///
/// The ordinary answer for a skill-only bundle, not a fault — see
/// [`crate::bundle::Bundles::fetch`].
pub(crate) fn bundle_missing() -> Error {
    Error::new(ErrorKind::BundleMissing)
}

/// Reports a deployment that never configured snapshot storage.
pub(crate) fn bundle_unconfigured() -> Error {
    Error::new(ErrorKind::BundleUnconfigured)
}

/// Reports an object store that was reached and would not serve.
///
/// The store's own error rides through as `#[source]` rather than being
/// stringified into a message: a refused signature, an unresolvable endpoint
/// and a missing bucket are three different operator problems, and the chain is
/// the only place that distinction survives (`RUST_ERROR_STANDARD` rule 3).
pub(crate) fn bundle_storage(source: object_store::Error) -> Error {
    Error::new(ErrorKind::BundleStorage { source })
}

/// Reports a stored object too large for this daemon to buffer.
///
/// Its own kind rather than a storage failure, because it is not one: the store
/// answered correctly and what it holds is the problem. The size is carried so
/// the operator's log line names it — nothing puts it on the wire.
pub(crate) fn bundle_oversized(size: u64) -> Error {
    Error::new(ErrorKind::BundleOversized { size })
}

/// Reports a statement that reached Postgres and was refused.
///
/// `map_err` that ADDS context the call site alone knows — which statement was
/// running — and nothing else. The `sqlx::Error` rides through as `#[source]`,
/// so the chain a fatal renderer walks stays intact (`RUST_ERROR_STANDARD`
/// rule 3).
pub(crate) fn query(context: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| Error::new(ErrorKind::Query { context, source })
}

/// Reports a column whose stored value is not a shape this daemon can read.
pub(crate) fn row_malformed(
    table: &'static str,
    column: &'static str,
) -> impl Fn(afd_core::error::Error) -> Error {
    move |source| {
        Error::new(ErrorKind::RowMalformed {
            table,
            column,
            source,
        })
    }
}

/// Reports a memory statement the durable store would not run.
///
/// The one raiser in this module that carries the caller's sentence, and the
/// reason is the surface it serves. `UZ-MEM-003` answers four different
/// operations — the role switch, the list, the search, the forget — and
/// `handler.zig` writes a different sentence for each, because the one thing a
/// reader of a 503 wants to know is which half of the surface is down.
/// `Rejected` above carries a detail for the same reason; what neither does is
/// let two sites describe ONE operation differently.
///
/// The `sqlx::Error` rides through as `#[source]`, so an operator still reads
/// the statement's own cause beside the request id.
pub(crate) fn memory_unavailable(detail: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| Error::new(ErrorKind::MemoryUnavailable { detail, source })
}
