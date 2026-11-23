//! The correlation token every HTTP response carries.
//!
//! `req_` followed by twelve lowercase hex characters. The shape is not an
//! internal detail: it is short enough for a person to read one off a browser
//! screenshot and type it into a support ticket, which is most of what it is
//! for. Lengthening it, or swapping it for a UUID, would break that use before
//! it broke any code.
//!
//! # One id per request
//!
//! [`drop_incoming`] removes any `x-request-id` a caller sent, `tower-http`'s
//! `SetRequestIdLayer` then stamps one minted through [`Mint`], and
//! `PropagateRequestIdLayer`
//! echoes it on the response. [`scope`] makes that id the one every refusal
//! built while the request is served reads through [`RequestId::current`], so
//! the header, the problem body and the log line name one request. Outside a
//! request — a spawned task, a suite calling a handler directly — `current`
//! mints a fresh id, as every refusal did before the header existed.

use std::fmt::{self, Display, Formatter};

use af_core::error_code;
use af_crypto::entropy::Entropy;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use http::HeaderValue;

/// The prefix that makes an id recognisable in a log line or a ticket.
const PREFIX: &str = "req_";

/// Random bytes behind one id: six, which hex-encode to exactly the twelve
/// characters after `PREFIX`.
const ENTROPY_BYTES: usize = 6;

/// The id used when entropy is unavailable.
///
/// A response is already being written when this is reached and an envelope
/// without a `request_id` would be a shape change, so the field says plainly
/// that there is no id rather than carrying an invented one that support would
/// then fail to find.
pub const UNKNOWN_REQUEST_ID: &str = "req_unknown";

tokio::task_local! {
    /// The id of the request this task is serving, set by [`scope`].
    static CURRENT: RequestId;
}

/// One request's correlation token.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RequestId(String);

impl RequestId {
    /// The id of the request being served: the one its `x-request-id` carries.
    ///
    /// A fresh id where no request is in scope.
    #[must_use]
    pub fn current() -> Self {
        CURRENT
            .try_with(Self::clone)
            .unwrap_or_else(|_| Self::mint())
    }

    /// A fresh id, drawing from the operating system.
    #[must_use]
    pub fn mint() -> Self {
        Self::mint_from(&Entropy::new())
    }

    /// A fresh id, drawing from `entropy`.
    ///
    /// Infallible on purpose. The caller is mid-response — there is nothing to
    /// return an error TO — so an entropy failure degrades to
    /// [`UNKNOWN_REQUEST_ID`].
    #[must_use]
    pub fn mint_from(entropy: &Entropy) -> Self {
        let mut bytes = [0u8; ENTROPY_BYTES];
        if entropy.fill(&mut bytes).is_err() {
            let code = error_code::INTERNAL_OPERATION_FAILED.as_str();
            // Logged at `error` on purpose. A host that cannot produce six
            // random bytes cannot seal a secret either, so the unnamed request
            // this degrades to is the least of what is about to go wrong — and
            // it is the first symptom that surfaces.
            {
                let (error_code, event, message) = (
                    &code,
                    &"request_id_entropy_unavailable",
                    format_args!(
                        "entropy unavailable — this request is unidentified, and \
                 anything that needs a key is about to fail"
                    ),
                );
                tracing::error!(message, error_code, event);
            };
            return Self(UNKNOWN_REQUEST_ID.to_owned());
        }
        Self(format!("{PREFIX}{}", hex::encode(bytes)))
    }

    /// The id as it appears on the wire.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for RequestId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<RequestId> for String {
    fn from(id: RequestId) -> Self {
        id.0
    }
}

/// The [`tower_http::request_id::MakeRequestId`] that mints this product's id.
#[derive(Debug, Clone, Copy, Default)]
pub struct Mint;

impl tower_http::request_id::MakeRequestId for Mint {
    fn make_request_id<B>(
        &mut self,
        _request: &http::Request<B>,
    ) -> Option<tower_http::request_id::RequestId> {
        HeaderValue::from_str(RequestId::mint().as_str())
            .ok()
            .map(tower_http::request_id::RequestId::new)
    }
}

/// Removes a caller's `x-request-id`: this daemon never accepts one.
///
/// Mounted in front of `SetRequestIdLayer`, which then always mints ours, so
/// every id a log line, a body or a support ticket carries is one we issued.
pub fn drop_incoming(mut request: Request) -> Request {
    request.headers_mut().remove(X_REQUEST_ID);
    request
}

/// The header a request id travels in.
const X_REQUEST_ID: &str = "x-request-id";

/// Serves `request` with its stamped id as [`RequestId::current`].
///
/// Mounted inside `SetRequestIdLayer`, which put the id in the extensions. An
/// id that is not visible ASCII cannot be written into a body, so the
/// refusals under it carry a fresh one instead.
pub async fn scope(request: Request, next: Next) -> Response {
    let id = request
        .extensions()
        .get::<tower_http::request_id::RequestId>()
        .and_then(|stamped| stamped.header_value().to_str().ok())
        .map_or_else(RequestId::mint, |stamped| RequestId(stamped.to_owned()));
    CURRENT.scope(id, next.run(request)).await
}

#[cfg(test)]
mod tests {
    use af_crypto::entropy::Entropy;

    use super::{RequestId, UNKNOWN_REQUEST_ID};

    /// The six bytes render in order, two lowercase digits each.
    #[test]
    fn the_id_is_its_entropy_in_byte_order() {
        let (entropy, control) = Entropy::new_mocked();
        control.push_bytes(&[0x01, 0x02, 0x03, 0x04, 0x05, 0xab]);

        // pin test: literal is the contract
        assert_eq!(RequestId::mint_from(&entropy).as_str(), "req_0102030405ab");
    }

    /// No entropy is an unidentified request, never a refused one.
    #[test]
    fn an_entropy_failure_is_the_unknown_id() {
        let (entropy, control) = Entropy::new_mocked();
        control.fail_next();

        assert_eq!(RequestId::mint_from(&entropy).as_str(), UNKNOWN_REQUEST_ID);
    }
}
