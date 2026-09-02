//! Where a person is sent once a connector callback has done its work.
//!
//! Two legs share one redirect: the browser leg relays the whole handoff to
//! the dashboard, and the dashboard leg lands the grant and sends the person
//! to the connector's page. What differs is what a redirect that cannot be
//! written MEANS on each leg, which is why the two entry points below exist
//! rather than one function with a flag.
//!
//! # A 200 on the completion leg, and a refusal on the relay leg
//!
//! On the completion leg the grant is sealed and the connection is live by
//! the time the destination is built, so failing the request would tell a
//! person their connect did not work when it did, and the next thing they
//! would do is press Connect again. `callback.zig` answers `200` with
//! `{"status":"connected"}` there for exactly that reason, and so does this.
//!
//! On the relay leg nothing has landed yet: the browser arrived carrying the
//! provider's code and this daemon could not name the dashboard page to hand
//! it to. That is the deployment's misconfiguration, the same one a dashboard
//! base that is not a URL raises, and it is refused as one rather than
//! answered as a connect that never happened.

use afd_connector::callback;
use afd_core::id::Uuid7;
use afd_wire::connector::{Connected, STATUS_CONNECTED};
use axum::Json;
use axum::response::{IntoResponse as _, Response};
use http::{StatusCode, header};

use super::unconfigured;
use crate::handler::Refusal;
use crate::services::Services;

/// The browser leg's redirect, or a refusal when it cannot be written.
///
/// # Errors
/// `UZ-CONN-001` when the destination cannot be a `Location` header.
pub(super) fn relayed(destination: &str) -> Result<Response, Refusal> {
    found(destination).ok_or_else(unconfigured)
}

/// Where a person lands once the connect has finished.
///
/// A redirect to the connector's page, or a `200` saying the grant landed
/// when no such page can be named.
pub(super) fn connected<D: Services>(services: &D, workspace: &Uuid7) -> Response {
    callback::connected_url(services.dashboard(), workspace)
        .and_then(|destination| found(&destination))
        .unwrap_or_else(sealed)
}

/// A redirect to `destination`, when it can be written as a header.
///
/// 302 rather than 303: the daemon this ports answers 302 on both callback
/// legs, and a browser follows either with a GET here because both arrive at a
/// destination that only serves one.
// `None` rather than a fallback response: a URL this daemon composed through
// `url` is already in the header's alphabet, so an unwritable one is a bug in
// that composer, and what to answer for it differs by leg.
fn found(destination: &str) -> Option<Response> {
    let location = header::HeaderValue::from_str(destination).ok()?;
    Some((StatusCode::FOUND, [(header::LOCATION, location)]).into_response())
}

/// The connect landed and there is nowhere to send the person.
fn sealed() -> Response {
    (
        StatusCode::OK,
        Json(Connected {
            status: STATUS_CONNECTED.into(),
        }),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use axum::response::Response;
    use http::{HeaderValue, StatusCode, header};

    use super::{found, sealed};

    /// A destination the header alphabet cannot carry is `None`, not a
    /// redirect to a truncated or mangled location.
    #[test]
    fn test_a_destination_the_header_cannot_carry_is_not_a_redirect() {
        assert!(found("https://app.example/x\ny").is_none());
    }

    /// A well-formed destination is a 302 carrying it verbatim.
    #[test]
    fn test_a_writable_destination_is_a_found_with_that_location() {
        let response = found("https://app.example/integrations");

        assert_eq!(
            response.as_ref().map(Response::status),
            Some(StatusCode::FOUND)
        );
        let location = response.and_then(|found| found.headers().get(header::LOCATION).cloned());
        assert_eq!(
            location.as_ref().map(HeaderValue::as_bytes),
            Some(&b"https://app.example/integrations"[..])
        );
    }

    /// The landed-but-nowhere-to-go answer is a 200 JSON document, never a
    /// bare status a client would type as returning nothing.
    #[tokio::test]
    async fn test_a_sealed_connect_answers_a_json_body_not_a_bare_status() {
        let response = sealed();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .map(HeaderValue::as_bytes),
            Some(&b"application/json"[..])
        );
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .ok();
        assert_eq!(bytes.as_deref(), Some(&br#"{"status":"connected"}"#[..]));
    }
}
