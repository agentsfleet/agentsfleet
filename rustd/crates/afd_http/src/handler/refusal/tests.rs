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
        (
            Refusal::missing_secrets(
                error_code::FLEET_BUNDLE_SECRETS_MISSING,
                "short a credential",
                vec!["github".to_owned()],
            ),
            StatusCode::FAILED_DEPENDENCY,
        ),
        (
            Refusal::already_resolved(
                error_code::APPROVAL_ALREADY_RESOLVED,
                "already answered",
                crate::envelope::Resolution {
                    gate_id: "g".to_owned(),
                    action_id: "a".to_owned(),
                    outcome: "approved".to_owned(),
                    resolved_at: 1,
                    resolved_by: "someone".to_owned(),
                },
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

/// The `current_state` a rendered refusal carries, when it carries one.
async fn current_state(refusal: Refusal) -> Option<String> {
    let body = axum::body::to_bytes(refusal.into_response().into_body(), usize::MAX)
        .await
        .ok()?;
    let document: serde_json::Value = serde_json::from_slice(&body).ok()?;
    document
        .get("current_state")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}

/// An error the call site names a state for is a conflict carrying it; any
/// other renders as `at` would, with no state a client could branch on.
#[tokio::test]
async fn conflict_or_at_names_a_state_only_for_the_errors_the_call_site_picks() {
    let named =
        Refusal::conflict_or_at("fixture", |_: &DomainRefusal| Some("paused"))(DomainRefusal);
    assert_eq!(current_state(named).await.as_deref(), Some("paused"));

    let plain = Refusal::conflict_or_at("fixture", |_: &DomainRefusal| None)(DomainRefusal);
    assert_eq!(current_state(plain).await, None);
}
