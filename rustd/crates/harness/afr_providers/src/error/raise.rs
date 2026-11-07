//! How a failure becomes an [`Error`](super::Error): the lift, and the
//! raisers for the kinds that carry data.

use garde::Validate as _;
use std::error::Error as StdError;

use rig_core::message::EmptyToolName;
use rig_core::{ProviderError, ProviderResponseError};

use super::{Error, ErrorKind};

/// The longest provider code a report carries; a longer one is a message.
const CODE_CAP: usize = 64;
/// The early end of a provider that named no code for it.
const UNNAMED_END: &str = "provider_error";

// The one lift: a body that will not encode, or an event that will not parse,
// is an unreadable turn wherever `?` meets it (`docs/RUST_ERROR_STANDARD.md`
// rule 2).
afd_core::error_lifts!(Error, ErrorKind:
    serde_json::Error => Unreadable,
);

/// The provider ended the turn with its own error, named `reason`.
pub(crate) fn ended(reason: &str, source: ProviderError) -> Error {
    Error::from(ErrorKind::Ended {
        reason: reason.to_owned(),
        source,
    })
}

/// A policy naming `provider`, which this runner does not speak.
pub(crate) fn unhosted(provider: &str) -> Error {
    Error::from(ErrorKind::Unhosted {
        provider: provider.to_owned(),
    })
}

/// A policy naming `provider`, a `custom:` endpoint at an address this runner
/// never dials.
pub(crate) fn blocked_endpoint(provider: &str) -> Error {
    Error::from(ErrorKind::BlockedEndpoint {
        provider: provider.to_owned(),
    })
}

/// The HTTP client could not be built, for `source`.
pub(crate) fn client(source: reqwest::Error) -> Error {
    Error::from(ErrorKind::Client { source })
}

/// Persisted model settings could not be admitted; preserve the typed cause.
pub(crate) fn configuration(source: afd_inference::Error) -> Error {
    Error::from(ErrorKind::Configuration { source })
}

/// The conversation cannot be sent: the result for `call_id` answers no call
/// before it.
pub(crate) fn unsendable(call_id: &str) -> Error {
    Error::from(ErrorKind::Unsendable {
        call_id: call_id.to_owned(),
    })
}

/// The conversation cannot be sent: call `call_id` names no tool, for
/// `source`.
pub(crate) fn unnamed(call_id: &str, source: EmptyToolName) -> Error {
    Error::from(ErrorKind::Unnamed {
        call_id: call_id.to_owned(),
        source,
    })
}

/// `failure`, as rig reported it, as the kind a report names: a status the
/// provider answered is a refusal under the provider's own code, a reply cut
/// short or a connection that dropped is lost, a provider's error after its
/// reply began ended the turn, a reply past the transport's cap is oversized,
/// and anything else is the wire's.
pub(crate) fn provider(failure: ProviderError) -> Error {
    let named = code(&failure);
    if let Some(status) = failure
        .provider_response_status()
        .filter(|status| !status.is_success())
    {
        let status = status.as_u16();
        return Error::from(ErrorKind::Refused {
            status,
            code: named,
            source: Some(failure),
        });
    }
    match failure {
        _ if oversize(&failure) => Error::from(ErrorKind::Oversize {
            source: failure,
            limit: afd_inference::transport::REPLY_MAX_BYTES,
        }),
        ProviderError::Http(_) | ProviderError::Truncated => Error::lost(failure),
        ProviderError::Provider(_) => ended(UNNAMED_END, failure),
        _ if failure.provider_response().is_some() => {
            ended(named.as_deref().unwrap_or(UNNAMED_END), failure)
        }
        source => Error::from(ErrorKind::Wire { source }),
    }
}

/// Whether the client's guard refused `failure`'s endpoint: its name resolves
/// to an address this runner never reaches, which no second send changes.
pub(crate) fn blocked(failure: &ProviderError) -> bool {
    let ProviderError::Http(transport) = failure else {
        return false;
    };
    let first: &(dyn StdError + 'static) = &**transport;
    afr_egress::blocked_address(first)
}

/// Whether the transport ended `failure`'s read for passing its cap. rig
/// keeps the transport's error behind its own and names no source for it, so
/// the walk starts there.
pub(crate) fn oversize(failure: &ProviderError) -> bool {
    let ProviderError::Http(transport) = failure else {
        return false;
    };
    let first: &(dyn StdError + 'static) = &**transport;
    std::iter::successors(Some(first), |&error| error.source())
        .filter_map(|error| error.downcast_ref::<afd_inference::Error>())
        .any(afd_inference::Error::is_oversize)
}

/// The provider's own name for `failure`, when it gave one that reads as a
/// name: the one spelling a report or a log line may carry.
pub(crate) fn code(failure: &ProviderError) -> Option<String> {
    failure
        .provider_response()
        .and_then(ProviderResponseError::machine_code)
        .filter(|code| is_code(code))
}

/// Whether `code` reads as a provider's name for a failure rather than its
/// message, which may quote the conversation and is never carried.
fn is_code(code: &str) -> bool {
    ProviderCode { value: code }.validate().is_ok()
}

#[derive(garde::Validate)]
struct ProviderCode<'a> {
    #[garde(length(bytes, min = 1, max = CODE_CAP), pattern(r"^[A-Za-z0-9_.:-]+$"))]
    value: &'a str,
}
