//! Parse protocol selection, held fleet identities and the poll's key before
//! any placement.

use afd_core::id::Uuid7;
use afd_http::handler::Refusal;
use afd_wire::lease::{AcceptedKinds, LeaseRequest};
use garde::Validate as _;
use http::HeaderMap;

/// The header a worker names one poll attempt with.
const POLL_KEY: &str = "idempotency-key";

/// What a malformed poll key is told.
const POLL_KEY_MALFORMED: &str = "Send at most one Idempotency-Key containing a UUIDv7";

/// The worker's key for this poll attempt, if it sent one.
///
/// Optional, so a runner that sends none polls as before. One header holding
/// one `UUIDv7`, or none: a second header or any other spelling is refused
/// before a claim runs, because a key the claim cannot match would silently
/// lose the replay it exists for.
pub(super) fn poll_key(headers: &HeaderMap) -> Result<Option<Uuid7>, Refusal> {
    let mut keys = headers.get_all(POLL_KEY).iter();
    let Some(first) = keys.next() else {
        return Ok(None);
    };
    if keys.next().is_some() {
        return Err(Refusal::malformed(POLL_KEY_MALFORMED));
    }
    first
        .to_str()
        .ok()
        .and_then(|value| Uuid7::parse(value).ok())
        .map(Some)
        .ok_or_else(|| Refusal::malformed(POLL_KEY_MALFORMED))
}

pub(super) struct Request {
    pub(super) held: Vec<Uuid7>,
    pub(super) accepted: AcceptedKinds,
}

pub(super) fn read(body: &[u8]) -> Result<Request, Refusal> {
    let request: LeaseRequest<'_> = afd_http::handler::read_body(body).map_err(|_invalid| {
        Refusal::malformed("accepted_kinds and valid polling input are required")
    })?;
    request
        .validate()
        .map_err(|_invalid| Refusal::malformed("Held fleets exceed their bounds"))?;
    let held = request
        .holds
        .0
        .iter()
        .map(|id| {
            Uuid7::parse(id)
                .map_err(|_invalid| Refusal::malformed("Held fleets must be UUIDv7 identities"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Request {
        held,
        accepted: request.accepted_kinds,
    })
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::expect_used,
        reason = "a test asserts by panicking; the restriction set is for the daemon"
    )]

    use axum::response::IntoResponse as _;
    use http::{HeaderMap, HeaderValue, StatusCode};

    use super::{POLL_KEY, poll_key};

    fn headers(values: &[&'static str]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for value in values {
            headers.append(POLL_KEY, HeaderValue::from_static(value));
        }
        headers
    }

    /// Dimension 5.5: a key that is not a `UUIDv7` is the standard 400
    /// problem, before any claim runs.
    #[test]
    fn test_lease_poll_rejects_a_malformed_idempotency_key() {
        for sent in [
            &["not-a-uuid"][..],
            // A version-4 UUID: well-formed, but not a key a worker mints.
            &["0b5d5c8e-6f0e-4e3a-9a5e-6a2f1d9b7c41"][..],
            &[
                "0195b4ba-8d3a-7000-8abc-000000000001",
                "0195b4ba-8d3a-7000-8abc-000000000002",
            ][..],
        ] {
            let refused = poll_key(&headers(sent)).expect_err("a malformed key is refused");
            assert_eq!(
                refused.into_response().status(),
                StatusCode::BAD_REQUEST,
                "{sent:?}"
            );
        }
    }

    /// No header is the poll as before, and one `UUIDv7` is read as given.
    #[test]
    fn a_missing_key_is_none_and_one_key_is_read() {
        let key = "0195b4ba-8d3a-7000-8abc-000000000001";
        assert_eq!(poll_key(&headers(&[])).ok(), Some(None));
        assert_eq!(
            poll_key(&headers(&[key]))
                .ok()
                .flatten()
                .map(|parsed| parsed.as_str().to_owned()),
            Some(key.to_owned())
        );
    }
}
