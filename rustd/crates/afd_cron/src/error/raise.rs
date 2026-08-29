//! How a failure becomes an [`Error`]: the lifts, and the raisers that bind data.
//!
//! Split from the type itself so [`super`] holds the vocabulary — the kinds,
//! and the one table pairing each with its code and sentence — while this holds
//! the ways to produce one.

use super::{Error, ErrorKind};

// Every lift is a `From`, so `?` does the conversion at the call site and no
// `map_err` appears on a path that adds nothing (`RUST_ERROR_STANDARD` rule 2).
// `sqlx::Error` is deliberately absent: a statement failure carries WHICH
// statement, which is context only the call site knows, so it goes through
// [`query`] instead of a blanket lift.
afd_core::error_lifts!(Error, ErrorKind:
    afd_db::Error => Datastore,
    afd_redis::Error => Queue,
    afd_crypto::error::Error => Identifier,
    afd_core::error::Error => IdentifierShape,
    reqwest::Error => UpstreamUnreachable,
);

/// Reports a statement that failed, naming what it was doing.
///
/// The `map_err` this crate keeps, and it earns its place by ADDING the
/// operation name — a fact the driver's error cannot carry and the call site
/// alone knows (`RUST_ERROR_STANDARD` rule 3). The `sqlx::Error` is kept as the
/// `source`, never stringified into a message.
pub(crate) fn query(context: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| ErrorKind::Query { context, source }.into()
}

/// Refuses a schedule row this build cannot make sense of, naming the column.
pub(crate) fn row_unreadable(column: &'static str) -> Error {
    ErrorKind::RowUnreadable { column }.into()
}

/// Reports an external scheduler whose answer this daemon could not read.
///
/// Classified as a REFUSAL rather than as unreachable, and the status is the
/// one the vendor actually sent: the call reached them and they answered, so
/// retrying it unchanged has no reason to do better. `0` is not a real status
/// and says so — there was an answer, and it was not one this build parses.
pub(crate) fn upstream_unreadable() -> Error {
    ErrorKind::UpstreamRefused { status: 0 }.into()
}

/// Reports an external scheduler that answered and refused.
///
/// A separate raiser from the `reqwest::Error` lift because it is a separate
/// FAILURE: a vendor that answered is reachable, and the thing to record is
/// what it said. See the module note on why collapsing the two would cost.
pub(crate) fn upstream_refused(status: u16) -> Error {
    ErrorKind::UpstreamRefused { status }.into()
}
