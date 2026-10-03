//! How a failure becomes an [`Error`](super::Error): the lift, and the
//! raisers for the kinds that carry data.

use super::{Error, ErrorKind};

// The one lift: a body that will not encode, or an event that will not parse,
// is an unreadable turn wherever `?` meets it (`docs/RUST_ERROR_STANDARD.md`
// rule 2).
afd_core::error_lifts!(Error, ErrorKind:
    serde_json::Error => Unreadable,
);

/// The provider ended the turn with its own error, named `reason`.
pub(crate) fn ended(reason: &str) -> Error {
    Error::from(ErrorKind::Ended {
        reason: reason.to_owned(),
    })
}

/// A policy naming `provider`, which this runner does not speak.
pub(crate) fn unhosted(provider: &str) -> Error {
    Error::from(ErrorKind::Unhosted {
        provider: provider.to_owned(),
    })
}

/// The HTTP client could not be built, for `source`.
pub(crate) fn client(source: reqwest::Error) -> Error {
    Error::from(ErrorKind::Client { source })
}
