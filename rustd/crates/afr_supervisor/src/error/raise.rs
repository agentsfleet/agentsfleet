//! How a failure becomes an [`Error`]: the lifts, and the raisers that bind data.

use std::borrow::Cow;

use afd_core::error_code::ErrorCode;
use serde::Deserialize;

use super::{Error, ErrorKind};
use crate::client::Verb;

// Every lift is a `From`, so `?` does the conversion and no `map_err` appears
// on a path that adds nothing (`docs/RUST_ERROR_STANDARD.md` rule 2). The
// client's and the decoder's errors are absent on purpose: both need to know
// WHICH verb failed, which only the call site can say. An address error has
// one meaning wherever it arises, so it lifts.
afd_core::error_lifts!(Error, ErrorKind:
    std::io::Error => Io,
    tempfile::PersistError => Persist,
    afd_core::error::Error => Identifier,
    tokio::task::JoinError => Task,
    url::ParseError => Address,
);

/// The one field of a problem body the runner reads.
#[derive(Deserialize)]
struct Problem<'a> {
    #[serde(borrow)]
    error_code: Option<Cow<'a, str>>,
}

/// Reports a setting the runner cannot start without.
pub(crate) fn config(detail: &'static str) -> Error {
    ErrorKind::Config { detail }.into()
}

/// Reports an HTTP client that could not be built.
pub(crate) fn client(source: reqwest::Error) -> Error {
    ErrorKind::Client { source }.into()
}

/// Reports a call that never completed a round trip.
pub(crate) fn transport(verb: Verb) -> impl Fn(reqwest::Error) -> Error {
    move |source| ErrorKind::Transport { verb, source }.into()
}

/// Reports a 5xx or a 429.
pub(crate) fn unavailable(verb: Verb, status: u16) -> Error {
    ErrorKind::Unavailable { verb, status }.into()
}

/// Reports a 4xx, carrying the code the daemon named.
pub(crate) fn refused(verb: Verb, status: u16, code: Option<ErrorCode>) -> Error {
    ErrorKind::Refused { verb, status, code }.into()
}

/// Reports a 4xx, reading the code out of its problem body.
///
/// A body that is not a problem document, or names a code this build does not
/// declare, refuses with no code rather than guessing one.
pub(crate) fn refused_with_body(verb: Verb, status: u16, body: &[u8]) -> Error {
    let code = afd_core::json::object_from_slice::<Problem<'_>>(body)
        .ok()
        .and_then(|problem| problem.error_code)
        .and_then(|named| ErrorCode::lookup(&named));
    refused(verb, status, code)
}

/// Reports a runner that stopped because the daemon refused its token.
pub(crate) fn token_refused() -> Error {
    ErrorKind::TokenRefused.into()
}

/// Reports a reply that did not decode as its verb's shape.
pub(crate) fn malformed(verb: Verb) -> impl Fn(serde_json::Error) -> Error {
    move |source| ErrorKind::Malformed { verb, source }.into()
}

/// Reports a request body that would not serialize.
pub(crate) fn encode(source: serde_json::Error) -> Error {
    ErrorKind::Encode { source }.into()
}

/// Reports bundle bytes that do not hash to the name they were fetched by.
pub(crate) fn tampered(content_hash: &str) -> Error {
    ErrorKind::BundleTampered {
        content_hash: content_hash.to_owned(),
    }
    .into()
}

/// Reports `repository` failing at `step`, with the git library's reason.
pub(crate) fn git(
    repository: &str,
    step: &'static str,
) -> impl FnOnce(Box<dyn std::error::Error + Send + Sync>) -> Error {
    move |source| {
        ErrorKind::Git {
            repository: repository.to_owned(),
            step,
            source,
        }
        .into()
    }
}
