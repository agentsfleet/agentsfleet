//! How a failure becomes an [`Error`]: the lifts, and the raisers that bind data.
//!
//! Split from the type itself so [`super`] holds the vocabulary — the kinds, and
//! the one table pairing each with its code and sentence — while this holds the
//! ways to produce one. A reader asking "what can go wrong here" and a reader
//! asking "where does this get raised" are looking for different things.

use super::{Error, ErrorKind};

afd_core::error_lifts!(Error, ErrorKind:
    afd_db::Error => Datastore,
    afd_redis::Error => Queue,
    afd_core::error::Error => Mint,
    afd_crypto::error::Error => Entropy,
    afd_fleet_runtime::Error => Config,
);

/// Reports a statement that failed, naming what it was doing.
pub(crate) fn query(context: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| ErrorKind::Query { context, source }.into()
}

/// Reports a column whose stored value is not one this build knows.
///
/// No `source`, and that is correct rather than lazy: nothing CAUSED the bytes
/// to be unknown — a newer daemon wrote a value this build has no variant for.
pub(crate) fn row_malformed(column: &'static str, stored: &str) -> Error {
    ErrorKind::RowMalformed {
        column,
        stored: stored.into(),
    }
    .into()
}

/// Reports a `SKILL.md` this daemon will not store.
///
/// Not a `#[from]`: [`ErrorKind::Config`] already claims that source type, and
/// which of the two documents failed is the whole difference between them.
pub(crate) fn skill(source: afd_fleet_runtime::Error) -> Error {
    ErrorKind::Skill { source }.into()
}

/// Refuses a conditional write against a version the row has moved past.
pub(crate) fn source_stale(current: impl Into<Box<str>>) -> Error {
    ErrorKind::SourceStale {
        current: current.into(),
    }
    .into()
}
