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
    afd_vault::Error => Vault,
    afd_crypto::error::Error => Entropy,
    afd_core::error::Error => IdentifierShape,
    reqwest::Error => VendorUnreachable,
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

/// Reports a provider that answered the exchange and refused it.
///
/// A separate raiser from the `reqwest::Error` lift because it is a separate
/// FAILURE: a vendor that answered is reachable, and what it said is the thing
/// worth recording. See the module note on why collapsing the two would cost.
pub(crate) fn exchange_refused(status: u16) -> Error {
    ErrorKind::ExchangeRefused { status }.into()
}

/// Reports a provider whose exchange body carries no grant this build can read.
pub(crate) fn exchange_unreadable() -> Error {
    ErrorKind::GrantUnreadable.into()
}
