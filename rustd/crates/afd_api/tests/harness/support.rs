//! Fixtures and the request helpers every suite sends through.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use afd_auth::credential::{CredentialKind, Presented};
use afd_auth::directory::{CredentialRecord, Liveness};
use afd_auth::mock::MockDirectory;
use afd_core::id::Uuid7;
use afd_redis::Redis;
use afd_redis::config::{RedisConfig, RedisRole};
use axum::Router;
use axum::body::Body;
use axum::response::Response;
use http::{Method, Request};
use serde_json::Value;
use tower::ServiceExt as _;

use std::time::Duration;

const REDIS_URL_KNOB: &str = "TEST_REDIS_URL";
const REDIS_CA_KNOB: &str = "TEST_REDIS_CA_CERT";

/// The integration lane's one Redis configuration.
pub(crate) fn redis_config() -> RedisConfig {
    let url = std::env::var(REDIS_URL_KNOB)
        .expect("TEST_REDIS_URL is set by make test-integration-rustd");
    RedisConfig::from_url(RedisRole::Default, url)
        .with_ca_cert_file(std::env::var(REDIS_CA_KNOB).ok().map(Into::into))
        .with_connect_timeout(Duration::from_secs(5))
        .with_request_timeout(Duration::from_secs(5))
}

/// A proven live connection using [`redis_config`].
pub(crate) async fn connect_redis() -> Redis {
    afd_redis::test_util::connect_live(&redis_config())
        .await
        .expect("the lane's Redis must be reachable")
}

/// The tenant every fixture person acts in.
pub(crate) fn tenant() -> Uuid7 {
    Uuid7::parse("019329c5-0000-7000-8000-000000000001").expect("the fixture tenant is canonical")
}

/// A runner identifier a fixture files a row under.
pub(crate) fn runner_id() -> Uuid7 {
    Uuid7::parse("019329c5-0000-7000-8000-0000000000a1").expect("the fixture runner is canonical")
}

/// Files a runner row, replacing whatever was under that credential.
///
/// Takes the directory by reference and clones inside, because `MockDirectory`
/// is a builder over shared state: `with` mutates the state every clone points
/// at and then hands the handle back. A suite revoking between two requests
/// wants the mutation and not the handle, and saying so once here keeps a
/// discarded return value out of every test that does it.
pub(crate) fn file_runner(directory: &MockDirectory, token: &str, runner: &Uuid7, live: Liveness) {
    let _filed = directory.clone().with(
        CredentialKind::RunnerToken,
        &presented(token),
        CredentialRecord::Machine {
            runner: runner.clone(),
            degraded: false,
            live,
        },
    );
}

/// A credential as the directory keys it — by the digest of what is PRESENTED,
/// so a fixture names the value a test will actually send.
pub(crate) fn presented(raw: &str) -> Presented {
    Presented::from_authorization(&format!("Bearer {raw}"))
        .expect("a fixture credential is never blank")
}

/// One request at `router`, with an optional credential.
pub(crate) async fn send(
    router: &Router,
    method: Method,
    path: &str,
    credential: Option<&str>,
    body: &str,
) -> Response {
    send_with_headers(router, method, path, credential, body, &[]).await
}

/// One request, carrying headers beyond the credential.
///
/// The conditional surfaces need this: an `If-Match` is the whole subject of
/// several cases, and it cannot be spelled through [`send`]. Everything else
/// goes through the shorter call, so there is one request builder rather than
/// two that could drift.
pub(crate) async fn send_with_headers(
    router: &Router,
    method: Method,
    path: &str,
    credential: Option<&str>,
    body: &str,
    headers: &[(http::HeaderName, &str)],
) -> Response {
    let mut request = Request::builder().method(method).uri(path);
    if let Some(token) = credential {
        request = request.header(http::header::AUTHORIZATION, format!("Bearer {token}"));
    }
    for (name, value) in headers {
        request = request.header(name, *value);
    }
    let request = request
        .body(Body::from(body.to_owned()))
        .expect("the test request is well formed");
    router
        .clone()
        .oneshot(request)
        .await
        .expect("axum is infallible")
}

/// Reads a response body back as JSON.
pub(crate) async fn json_body(response: Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("a test response body is small and in memory");
    serde_json::from_slice(&bytes).expect("the response must be valid JSON")
}
