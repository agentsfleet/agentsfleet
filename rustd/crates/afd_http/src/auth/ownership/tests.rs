#![expect(
    clippy::expect_used,
    reason = "test fixtures and extractor assertions should fail loudly"
)]

use afd_auth::principal::{Principal, Runner};
use afd_core::id::Uuid7;
use axum::extract::FromRequestParts as _;

use super::{Acting, Owned, WorkspaceContext};

fn id(suffix: &str) -> Uuid7 {
    Uuid7::parse(&format!("0195b4ba-8d3a-7f13-8abc-{suffix}"))
        .expect("fixture identifier is UUIDv7")
}

#[tokio::test]
async fn workspace_context_requires_the_ownership_verdict() {
    let expected = Owned {
        workspace: id("2b3e1e0c1011"),
        tenant: id("2b3e1e0c1012"),
    };
    let (mut parts, _body) = http::Request::new(()).into_parts();
    parts.extensions.insert(expected.clone());
    let extracted = WorkspaceContext::from_request_parts(&mut parts, &())
        .await
        .expect("ownership verdict extracts");
    assert_eq!(extracted.0, expected);

    let (mut missing, _body) = http::Request::new(()).into_parts();
    let response = WorkspaceContext::from_request_parts(&mut missing, &())
        .await
        .expect_err("a context without its ownership layer fails closed");
    assert_eq!(response.status(), http::StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn acting_requires_the_principal_installed_by_the_guard() {
    let principal = Principal::Runner(Runner::new(id("2b3e1e0c1013"), false));
    let (mut parts, _body) = http::Request::new(()).into_parts();
    parts.extensions.insert(principal.clone());
    let extracted = Acting::from_request_parts(&mut parts, &())
        .await
        .expect("proven principal extracts");
    assert_eq!(extracted.0, principal);

    let (mut missing, _body) = http::Request::new(()).into_parts();
    let response = Acting::from_request_parts(&mut missing, &())
        .await
        .expect_err("an acting extractor without its guard fails closed");
    assert_eq!(response.status(), http::StatusCode::INTERNAL_SERVER_ERROR);
}
