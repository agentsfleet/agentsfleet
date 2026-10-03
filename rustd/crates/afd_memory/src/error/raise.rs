//! How a failure becomes an [`Error`]: the lifts, and the raisers that bind data.

use super::{Error, ErrorKind};

// `sqlx::Error` is absent on purpose: a statement failure carries WHICH
// statement, context only the call site knows, so it goes through [`query`]
// or [`unavailable`] instead of a blanket lift.
afd_core::error_lifts!(Error, ErrorKind:
    afd_db::Error => Datastore,
    afd_crypto::error::Error => Identifier,
    afd_core::error::Error => Writer,
);

/// Reports a runner-plane statement that failed, naming what it was doing.
pub(crate) fn query(context: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| ErrorKind::Query { context, source }.into()
}

/// Reports an operator-surface statement the memory store would not run, with
/// the sentence naming the operation.
pub(crate) fn unavailable(detail: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| ErrorKind::Unavailable { detail, source }.into()
}

/// Refuses a fleet the named workspace does not hold, or that does not exist —
/// one answer for both, so this is no oracle for which identifiers are real.
pub(crate) fn fleet_not_found() -> Error {
    ErrorKind::FleetNotFound.into()
}

/// Refuses a forget of a key the fleet is not holding.
pub(crate) fn entry_not_found() -> Error {
    ErrorKind::EntryNotFound.into()
}

/// Refuses a call that would race a flip's copy into `store`.
pub(crate) fn moving(store: &'static str) -> Error {
    ErrorKind::Moving { store }.into()
}
