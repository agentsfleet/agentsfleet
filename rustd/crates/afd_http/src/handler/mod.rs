//! Shared request parsing and refusal rendering for every API plane.

pub mod encoding;
pub mod library_onboard;
pub use self::encoding::BrokenEscape;
mod refusable;
mod refusal;

use afd_observability::metrics::label::library::ReadOutcome;
use http::StatusCode;
use std::borrow::Cow;

use axum::response::{IntoResponse as _, Response};

pub use self::refusable::{Refusable, refuse};
pub use self::refusal::Refusal;

/// The event a refused unknown field is logged under.
const EVENT_UNKNOWN_FIELD: &str = "request_field_unknown";

/// Reads a request body into `T`, naming the offending key when a closed type
/// refuses an unknown one.
///
/// # Why this exists beside [`afd_core::json::object_from_slice`]
///
/// Every wire type is closed — `deny_unknown_fields` — so a caller one release
/// ahead, or one typo away, gets a refusal. Handlers map that to a fixed
/// `malformed` sentence, which is the right answer to give a caller and a
/// useless one to debug from: the daemon knew exactly which key it disliked and
/// then said only "malformed JSON". This logs the key on the way past, so an
/// operator can see `request_field_unknown field=wire_version` instead of
/// diffing structs by hand.
///
/// # What is never logged
///
/// Only the NAME, and only for an unknown-field refusal. Every other failure
/// logs nothing at all, because `serde_json` renders a type refusal with the
/// VALUE it rejected — `invalid type: string "sk-live-…", expected u32` — and
/// these bodies carry api keys and minted tokens.
/// [`afd_core::json::unknown_field_of`] is the filter that enforces the split,
/// and bounds the name, which the caller chose.
///
/// # Errors
/// Returns `serde_json`'s own error unchanged, so a call site keeps whatever it
/// already does with the failure and the `source()` chain survives.
pub fn read_body<'de, T>(body: &'de [u8]) -> Result<T, serde_json::Error>
where
    T: serde::Deserialize<'de>,
{
    afd_core::json::object_from_slice(body).inspect_err(|failure| {
        if let Some(field) = afd_core::json::unknown_field_of(failure) {
            tracing::warn!(
                event = EVENT_UNKNOWN_FIELD,
                field = field,
                "a request named a field this build does not carry; it was refused"
            );
        }
    })
}

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

/// Returns one query parameter with URL percent escapes decoded.
///
/// # Errors
/// Returns [`BrokenEscape`] for incomplete or non-hex escapes and for decoded
/// bytes that are not UTF-8.
pub fn decoded_parameter<'q>(
    query: &'q str,
    name: &str,
) -> Result<Option<Cow<'q, str>>, BrokenEscape> {
    parameter(query, name)
        .map(encoding::decode_form)
        .transpose()
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
    use super::{ReadOutcome, library_outcome, read_body};
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

    /// A closed request type, the shape every wire type now has.
    #[derive(Debug, serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Closed {
        api_key: String,
    }

    /// The happy path hands the value back and logs nothing.
    #[test]
    fn a_readable_body_deserializes_unchanged() {
        let read: Closed =
            read_body(br#"{"api_key":"sk-live-secret"}"#).expect("a well-formed body is read");
        assert_eq!(
            read.api_key, "sk-live-secret",
            "the value arrives as sent — this wrapper inspects the error, never the success"
        );
    }

    /// An unknown key is refused, and the error the caller keeps is `serde`'s own.
    ///
    /// The log line itself is the side effect this wrapper exists for and no
    /// subscriber is installed here to capture it. What IS asserted is the half
    /// that can go wrong silently: that the refusal still arrives at the call
    /// site unchanged, so a handler's `else` arm keeps firing.
    #[test]
    fn an_unknown_field_is_still_refused_to_the_caller() {
        let refused = read_body::<Closed>(br#"{"api_key":"sk-live-secret","wire_version":2}"#)
            .expect_err("a closed type refuses a key it does not carry");
        assert!(
            refused
                .to_string()
                .starts_with("unknown field `wire_version`"),
            "the caller receives serde's own error, not a rewritten one: {refused}"
        );
    }

    /// A type refusal is passed through WITHOUT the name filter matching.
    ///
    /// This is the arm that protects the credential: `serde` renders this one
    /// with the rejected VALUE in it, so it must take the branch that logs
    /// nothing. The assertion is that the value is present in the error the
    /// caller holds — which is exactly why it may not be logged.
    #[test]
    fn a_type_refusal_is_returned_but_carries_the_value_it_rejected() {
        let refused = read_body::<Closed>(br#"{"api_key":123}"#)
            .expect_err("a number is not a string and the type refuses it");
        assert!(
            !refused.to_string().starts_with("unknown field `"),
            "a type refusal is not an unknown-field refusal, so the name filter \
             answers None and nothing is logged: {refused}"
        );
    }
}
