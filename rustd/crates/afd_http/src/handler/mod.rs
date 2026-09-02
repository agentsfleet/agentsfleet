//! Shared request parsing and refusal rendering for every API plane.

pub mod library_onboard;
mod refusable;
mod refusal;

use afd_observability::metrics::label::library::ReadOutcome;
use http::StatusCode;
use std::borrow::Cow;

use axum::response::{IntoResponse as _, Response};

pub use self::refusable::{Refusable, refuse};
pub use self::refusal::Refusal;

/// Refuses a request this daemon cannot read at all.
#[must_use]
pub fn malformed(detail: &'static str) -> Response {
    reject(afd_core::error_code::INVALID_REQUEST, detail)
}

/// Writes a registry refusal whose detail only the call site knows.
#[must_use]
pub fn reject(code: afd_core::error_code::ErrorCode, detail: &'static str) -> Response {
    crate::envelope::ProblemResponse::new(code, detail, crate::request_id::RequestId::mint())
        .into_response()
}

/// Returns one raw query-string parameter by name.
#[must_use]
pub fn parameter<'q>(query: &'q str, name: &str) -> Option<&'q str> {
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then_some(value)
    })
}

/// A broken percent escape or a decoded value that is not UTF-8.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BrokenEscape;

/// Returns one query parameter with URL percent escapes decoded.
///
/// # Errors
/// Returns [`BrokenEscape`] for incomplete or non-hex escapes and for decoded
/// bytes that are not UTF-8.
pub fn decoded_parameter<'q>(
    query: &'q str,
    name: &str,
) -> Result<Option<Cow<'q, str>>, BrokenEscape> {
    let Some(raw) = parameter(query, name) else {
        return Ok(None);
    };
    if !raw.bytes().any(|byte| byte == b'%' || byte == b'+') {
        return Ok(Some(Cow::Borrowed(raw)));
    }
    let bytes = raw.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        match byte {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' => {
                let high = bytes.get(index + 1).copied().and_then(hex_value);
                let low = bytes.get(index + 2).copied().and_then(hex_value);
                let (Some(high), Some(low)) = (high, low) else {
                    return Err(BrokenEscape);
                };
                decoded.push(high << 4 | low);
                index += 3;
            }
            other => {
                decoded.push(other);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded)
        .map(Cow::Owned)
        .map(Some)
        .map_err(|_not_text| BrokenEscape)
}

const fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// The refusal a path segment naming no shipped connector earns.
///
/// `registry.zig`'s `UNKNOWN_PROVIDER_DETAIL_FALLBACK`.
const DETAIL_UNKNOWN_PROVIDER: &str = "Unknown connector";

/// The provider a path segment names.
///
/// Substrate rather than plane: the tenant plane parses this segment on the
/// connect and status surfaces, and the ingress plane parses the same segment
/// on the events route. One parse, so the two planes agree on what a provider
/// segment means without depending on each other.
///
/// # Errors
/// `UZ-CONN-004` for a segment this daemon ships no connector for. A refusal
/// rather than a 404 with no code, because the caller is a dashboard rendering
/// a card and the code is what tells it the card is stale.
pub fn provider_of(segment: &str) -> Result<afd_connector::Provider, Refusal> {
    afd_connector::Provider::parse(segment).ok_or_else(|| {
        Refusal::coded(
            afd_core::error_code::CONNECTOR_UNKNOWN,
            DETAIL_UNKNOWN_PROVIDER,
        )
    })
}

/// The fleet named in a path, still text.
///
/// Substrate rather than plane: the tenant plane extracts it on every
/// fleet-scoped route and the ingress plane extracts the same segment on every
/// per-fleet webhook, so one extractor keeps the two planes agreeing without
/// depending on each other.
#[derive(Debug, serde::Deserialize)]
pub struct FleetPath {
    /// The fleet named in the path, still text.
    pub fleet_id: String,
}

/// The refusal a path segment that is not an identifier earns.
pub const DETAIL_FLEET_ID: &str = "fleet_id must be a valid UUIDv7";

/// The fleet a path segment names.
///
/// # Errors
/// A malformed refusal for a segment that is not a `UUIDv7`, so the `::uuid`
/// cast in the statements below is never the thing that fails.
pub fn parse_fleet_id(raw: &str) -> Result<afd_core::id::Uuid7, Refusal> {
    afd_core::id::Uuid7::parse(raw)
        .map_err(|_not_an_identifier| Refusal::malformed(DETAIL_FLEET_ID))
}

/// How a refused library read is classified for the outcome family.
///
/// Off the STATUS rather than the registry code, and the loss is named: two
/// refusals that share a status are counted alike. That is the right trade for
/// this family — the question it answers is "what is failing these reads", and
/// the status is what separates a caller's mistake from a dependency's.
///
/// Anything unclassified lands on [`ReadOutcome::InternalError`], never on
/// `Ok`: a path that ends in a status this map does not know is one to
/// investigate, and calling it a success is how it would stay unnoticed.
#[must_use]
pub fn library_outcome(refusal: &Refusal) -> ReadOutcome {
    match refusal.status() {
        StatusCode::BAD_REQUEST | StatusCode::UNPROCESSABLE_ENTITY => ReadOutcome::Invalid,
        StatusCode::UNAUTHORIZED => ReadOutcome::Unauthorized,
        StatusCode::FORBIDDEN => ReadOutcome::Forbidden,
        StatusCode::NOT_FOUND => ReadOutcome::NotFound,
        StatusCode::REQUEST_TIMEOUT | StatusCode::GATEWAY_TIMEOUT => ReadOutcome::Timeout,
        StatusCode::BAD_GATEWAY | StatusCode::SERVICE_UNAVAILABLE => ReadOutcome::DependencyError,
        _unclassified => ReadOutcome::InternalError,
    }
}

#[cfg(test)]
mod tests {
    use super::{ReadOutcome, library_outcome};
    use crate::handler::refusal::Refusal;
    use afd_core::error_code;

    /// A status this map does not know is an internal error, never an `Ok`.
    ///
    /// The fallthrough is the arm that matters: the family exists to answer
    /// "what is failing these reads", and a refusal counted as a SUCCESS is one
    /// that never appears in that answer. So an unmapped status has to land on
    /// something an operator investigates, and the mapped ones have to keep
    /// their own classification rather than all falling here.
    #[test]
    fn an_unmapped_status_is_an_internal_error_rather_than_a_success() {
        let internal = Refusal::coded(
            error_code::INTERNAL_OPERATION_FAILED,
            "the dependency did not answer",
        );
        assert_eq!(
            library_outcome(&internal),
            ReadOutcome::InternalError,
            "a status outside the map is investigated, not counted as served"
        );

        let not_found = Refusal::coded(error_code::LIBRARY_INPUT_OUT_OF_BOUNDS, "out of bounds");
        assert_ne!(
            library_outcome(&not_found),
            ReadOutcome::InternalError,
            "a status the map DOES know keeps its own classification"
        );
    }
}
