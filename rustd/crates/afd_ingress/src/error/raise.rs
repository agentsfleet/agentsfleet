//! How a failure becomes an [`Error`]: the lifts, and the raiser that binds data.
//!
//! Split from the type itself so [`super`] holds the vocabulary — the kinds,
//! and the one table pairing each with its code and sentence — while this holds
//! the ways to produce one. A reader asking "what can go wrong here" and a
//! reader asking "where does this get raised" are looking for different things.

use super::{Error, ErrorKind};

// Every lift is a `From`, so `?` does the conversion at the call site and no
// `map_err` appears on a path that adds nothing (`RUST_ERROR_STANDARD` rule 2).
// `sqlx::Error` is deliberately absent: a statement failure carries WHICH
// statement, which is context only the call site knows, so it goes through
// [`query`] instead of a blanket lift.
afd_core::error_lifts!(Error, ErrorKind:
    afd_db::Error => Datastore,
    afd_vault::Error => Vault,
    afd_redis::Error => Queue,
    afd_fleet_runtime::Error => ConfigUnreadable,
);

/// Reports a statement that failed, naming what it was doing.
///
/// The one `map_err` this crate keeps, and it earns its place by ADDING the
/// operation name — a fact the driver's error cannot carry and the call site
/// alone knows (`RUST_ERROR_STANDARD` rule 3). The `sqlx::Error` is kept as the
/// `source`, never stringified into a message.
pub(crate) fn query(context: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| ErrorKind::Query { context, source }.into()
}

/// Refuses a fleet row this build cannot make sense of, naming the column.
///
/// Not a default, for the reason [`afd_fleet_lifecycle::FleetStatus::parse`]
/// gives about its own half: a newer daemon's status read as `installing` here
/// would let this one act on a state it does not understand. The delivery is
/// refused and the column is named, so an operator has somewhere to look.
pub(crate) fn row_unreadable(column: &'static str) -> Error {
    ErrorKind::RowUnreadable { column }.into()
}
