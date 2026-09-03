//! What each refusal constructor puts on the wire.
//!
//! Split from [`super`] at the file cap's first cut — a module's inline tests
//! and its fixtures are what move first, because they free the most lines for
//! the least risk.

use super::*;
use http::StatusCode;

#[derive(Debug)]
struct DomainRefusal;

impl Refusable for DomainRefusal {
    fn code(&self) -> afd_core::error_code::ErrorCode {
        error_code::AGENTSFLEET_PAUSED_INGRESS
    }

    fn detail(&self) -> &'static str {
        "the fleet is paused"
    }

    fn is_datastore_unavailable(&self) -> bool {
        false
    }

    fn reason(&self) -> String {
        "fixture refusal".to_owned()
    }
}

#[test]
fn constructors_preserve_each_refusal_status_and_header() {
    let cases = [
        (Refusal::at("fixture")(DomainRefusal), StatusCode::CONFLICT),
        (Refusal::malformed("malformed"), StatusCode::BAD_REQUEST),
        (
            Refusal::coded(error_code::INVALID_REQUEST, "coded"),
            StatusCode::BAD_REQUEST,
        ),
        (
            Refusal::conflict(error_code::AGENTSFLEET_PAUSED_INGRESS, "paused", "paused"),
            StatusCode::CONFLICT,
        ),
        (Refusal::forbidden("forbidden"), StatusCode::FORBIDDEN),
        (
            Refusal::unauthorized("unauthorized"),
            StatusCode::UNAUTHORIZED,
        ),
        (
            Refusal::preconditioned(error_code::AGENTSFLEET_SOURCE_STALE, "stale", "tag"),
            StatusCode::PRECONDITION_FAILED,
        ),
        (
            Refusal::conflict_at("fixture", "paused")(DomainRefusal),
            StatusCode::CONFLICT,
        ),
        (
            Refusal::conflict_detailed("fixture", "counted conflict".to_owned(), "paused")(
                DomainRefusal,
            ),
            StatusCode::CONFLICT,
        ),
    ];

    for (refusal, expected) in cases {
        assert_eq!(refusal.into_response().status(), expected);
    }

    let ceiling = Refusal::at_stream_ceiling(2, 2).into_response();
    assert_eq!(ceiling.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        ceiling.headers().get(header::RETRY_AFTER),
        Some(&HeaderValue::from_static("1"))
    );
}
