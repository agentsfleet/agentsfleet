//! Where a person is sent once a connector callback has done its work.
//!
//! # A 200 when no page can be named
//!
//! The grant is sealed and the connection is live by the time the destination
//! is built, so failing the request would tell a person their connect did not
//! work when it did, and the next thing they would do is press Connect again.
//! So this answers `200` with `{"status":"connected"}` instead.

use afd_api_wire::connector::{Connected, STATUS_CONNECTED};
use afd_connector::callback;
use afd_core::id::Uuid7;
use axum::Json;
use axum::response::{IntoResponse as _, Response};
use http::{StatusCode, header};

use crate::services::Services;

/// Where a person lands once the connect has finished.
///
/// A redirect to the connector's page, or a `200` saying the grant landed
/// when no such page can be named.
pub(super) fn connected<D: Services>(services: &D, workspace: &Uuid7) -> Response {
    found(&callback::connected_url(services.dashboard(), workspace)).unwrap_or_else(sealed)
}

/// A redirect to `destination`, when it can be written as a header.
///
/// 302: a browser follows it with a GET, and the destination serves only GET.
// `None` rather than a fallback response: a URL this daemon composed through
// `url` is already in the header's alphabet, so an unwritable one is a bug in
// that composer, answered by [`sealed`].
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
