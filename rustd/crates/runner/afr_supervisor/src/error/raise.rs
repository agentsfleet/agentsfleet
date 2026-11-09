//! How a failure becomes an [`Error`]: the lifts, and the raisers that bind data.

use std::borrow::Cow;

use af_core::error_code::ErrorCode;
use serde::Deserialize;

use super::{Error, ErrorKind};
use crate::client::Verb;

// Every lift is a `From`, so `?` does the conversion and no `map_err` appears
// on a path that adds nothing (`docs/RUST_ERROR_STANDARD.md` rule 2). The
// client's and the decoder's errors are absent on purpose: both need to know
// WHICH verb failed, which only the call site can say. An address error has
// one meaning wherever it arises, so it lifts.
af_core::error_lifts!(Error, ErrorKind:
    af_stream::Error => Stream,
    tokio::time::error::Elapsed => StreamTimeout,
    std::io::Error => Io,
    af_crypto::error::Error => Crypto,
    af_core::error::Error => Identifier,
    tokio::task::JoinError => Task,
    url::ParseError => Address,
    garde::Report => LeaseSize,
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

/// Preserve the transport cause without logging temporary storage credentials.
pub(crate) fn checkpoint_transfer(operation: &'static str) -> impl FnOnce(reqwest::Error) -> Error {
    move |source| {
        ErrorKind::CheckpointTransfer {
            operation,
            source: source.without_url(),
        }
        .into()
    }
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
    let code = af_core::json::object_from_slice::<Problem<'_>>(body)
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

/// Reports an egress host the resolver could not resolve.
pub(crate) fn egress_unresolved(host: &str) -> impl FnOnce(std::io::Error) -> Error {
    move |source| {
        ErrorKind::EgressUnresolved {
            host: host.to_owned(),
            source,
        }
        .into()
    }
}

/// Reports an egress host that resolved to IPv6 addresses alone.
pub(crate) fn egress_no_ipv4(host: &str) -> Error {
    ErrorKind::EgressNoIpv4 {
        host: host.to_owned(),
    }
    .into()
}

/// Reports a fleet egress host that resolved to an address no fleet may
/// reach.
pub(crate) fn egress_blocked(host: &str) -> Error {
    ErrorKind::EgressBlocked {
        host: host.to_owned(),
    }
    .into()
}

/// Reports a resolved allowlist the sandbox engine would not take.
pub(crate) fn egress(source: afr_sandbox::Error) -> Error {
    ErrorKind::Egress { source }.into()
}

/// Retains a native recovery failure with its operation and original cause.
pub(crate) fn boundary(
    detail: &'static str,
    source: impl std::error::Error + Send + Sync + 'static,
) -> Error {
    ErrorKind::Boundary {
        detail,
        source: Box::new(source),
    }
    .into()
}

/// Preserve a native channel failure without changing its retry classification.
pub(crate) fn stream_disconnected(
    source: impl std::error::Error + Send + Sync + 'static,
) -> super::Error {
    super::ErrorKind::StreamDisconnected {
        source: Box::new(source),
    }
    .into()
}

/// Preserve the cause of a refused native capacity or delivery invariant.
pub(crate) fn stream_protocol(
    source: impl std::error::Error + Send + Sync + 'static,
) -> super::Error {
    super::ErrorKind::StreamProtocolSource {
        source: Box::new(source),
    }
    .into()
}
