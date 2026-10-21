//! The raisers that REPORT something went wrong.
//!
//! The mirror image of [`super::refuse`], split from [`super`] along the line
//! that module's own header draws. A refusal there says the system worked and
//! the answer is no; every constructor here says the system did not work — a
//! statement Postgres would not run, a column holding a value this daemon
//! cannot read, an object store that answered with a fault, an admission the
//! producer wrote wrong.
//!
//! Splitting them was the file cap's doing and it improved the module: what is
//! left in [`super`] is now the type, the alias and the kinds, so a reader
//! looking for what a failure IS no longer scrolls past ninety lines of how one
//! is built. Every name here is re-exported from [`super`], so no call site
//! spells this module.

use super::{Error, ErrorKind};

/// Refuses a request the caller can correct, quoting the detail the call site
/// chose from `detail.rs`.
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
