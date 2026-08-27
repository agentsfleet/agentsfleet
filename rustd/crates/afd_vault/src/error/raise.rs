//! How a failure becomes an [`Error`]: the lifts, and the raisers that bind data.
//!
//! Split from the type itself so [`super`] holds the vocabulary — the kinds, and
//! the one table pairing each with its code and sentence — while this holds the
//! ways to produce one. A reader asking "what can go wrong here" and a reader
//! asking "where does this get raised" are looking for different things.

use super::{Error, ErrorKind};

/// Lifts a foreign error into its variant, so `?` needs no arm at a call site.
///
/// Per source type rather than one blanket over `Into<ErrorKind>`, which would
/// collide with the standard library's reflexive `From<T> for T`.
macro_rules! lifts {
    ($($source:ty => $variant:ident),+ $(,)?) => {
        $(impl From<$source> for Error {
            fn from(source: $source) -> Self {
                ErrorKind::$variant { source }.into()
            }
        })+
    };
}

lifts! {
    afd_db::Error => Datastore,
    afd_crypto::error::Error => Crypto,
    afd_core::error::Error => Mint,
}

/// Reports a statement that failed, naming what it was doing.
pub(crate) fn query(context: &'static str) -> impl Fn(sqlx::Error) -> Error {
    move |source| ErrorKind::Query { context, source }.into()
}

/// Refuses a delete over credentials the model registry still names.
///
/// The count comes from the statement that took the entry locks, so it is
/// stable for the life of the transaction that raised this — see
/// [`crate::delete`].
pub(crate) fn still_referenced(count: u32) -> Error {
    ErrorKind::StillReferenced { count }.into()
}
