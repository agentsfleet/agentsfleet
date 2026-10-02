//! Fixtures and the request helpers every suite sends through.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use afd_auth::credential::{CredentialKind, Presented};
use afd_auth::directory::{CredentialRecord, Liveness};
use afd_auth::mock::MockDirectory;
use afd_core::id::Uuid7;
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};
use afd_dragonfly::{Dragonfly, SubscriptionHub};
use axum::Router;
use axum::body::Body;
use axum::response::Response;
use http::{Method, Request, StatusCode};
use serde_json::Value;
use tower::ServiceExt as _;

use std::time::Duration;

/// The refusal envelope's registry-code field.
///
/// `problem_json_envelope.rs` pins the shape; suites that compare one refusal
/// against another read the code through this name, so a test cannot quietly
/// compare two absent fields and pass.
pub(crate) const ERROR_CODE: &str = "error_code";

/// The field a page envelope carries its rows under.
const ITEMS: &str = "items";

const DRAGONFLY_URL_KNOB: &str = "TEST_DRAGONFLY_URL";
const DRAGONFLY_CA_KNOB: &str = "TEST_DRAGONFLY_CA_CERT";

/// The integration lane's one Dragonfly configuration.
pub(crate) fn dragonfly_config() -> DragonflyConfig {
    let url = std::env::var(DRAGONFLY_URL_KNOB)
        .expect("TEST_DRAGONFLY_URL is set by make test-integration-rustd");
    DragonflyConfig::from_url(DragonflyRole::Default, url)
        .with_ca_cert_file(std::env::var(DRAGONFLY_CA_KNOB).ok().map(Into::into))
        .with_connect_timeout(Duration::from_secs(5))
        .with_request_timeout(Duration::from_secs(5))
}

/// A proven live connection using [`dragonfly_config`].
pub(crate) async fn connect_redis() -> Dragonfly {
    afd_dragonfly::test_util::connect_live(&dragonfly_config())
        .await
        .expect("the lane's Dragonfly must be reachable")
}

/// The subscription hub a live stream reads through, over [`dragonfly_config`].
///
/// The caller shuts it down: a hub left running holds the lane's subscription
/// connection past the test that opened it.
pub(crate) async fn live_hub() -> SubscriptionHub {
    SubscriptionHub::start(dragonfly_config())
        .await
        .expect("the lane's subscription connection starts")
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

/// One request through [`send`], answered as its status and its JSON body.
///
/// An empty body reads as `null`, which is what a `204` carries. Any other
/// body must parse: a route that stops answering JSON fails here instead of
/// reading as a refusal with no code.
pub(crate) async fn exchange(
    router: &Router,
    method: Method,
    path: &str,
    credential: Option<&str>,
    body: &str,
) -> (StatusCode, Value) {
    let response = send(router, method, path, credential, body).await;
    let status = response.status();
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("a test response body is small and in memory");
    let answered = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).expect("a response with a body answers JSON")
    };
    (status, answered)
}

/// The registry code a refusal carries, read through [`ERROR_CODE`].
pub(crate) fn error_code(problem: &Value) -> Option<&str> {
    problem.get(ERROR_CODE).and_then(Value::as_str)
}

/// A string field of a JSON object, when it holds one.
///
/// `None` for an absent key or a value of another type, so a suite asserts
/// presence where it needs a value rather than reading an empty string.
pub(crate) fn text<'v>(value: &'v Value, key: &str) -> Option<&'v str> {
    value.get(key).and_then(Value::as_str)
}

/// The rows a page carries.
///
/// Strict on purpose: a page with no `items` array is a broken answer, and
/// reading it as an empty one would let an emptiness assertion pass over it.
pub(crate) fn items(page: &Value) -> &[Value] {
    page.get(ITEMS)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .expect("a page carries an items array")
}

/// What every path parameter is filled with while probing.
///
/// A UUID rather than a word, so a substitution can never collide with a
/// literal sibling segment: `/v1/auth/sessions/{session_id}` and
/// `/v1/auth/sessions/all` are different routes, and a placeholder spelled
/// `all` would silently probe the wrong one.
const PARAMETER_FILL: &str = "00000000-0000-7000-8000-000000000000";

/// A concrete path for `template`, with every `{parameter}` filled.
///
/// `matchit` matches any non-empty segment against a parameter, so the value
/// only has to be non-empty and free of `/`. `workspace`, when given, fills
/// `{workspace_id}` instead, for a suite whose ownership stub owns one.
pub(crate) fn concrete_path(template: &str, workspace: Option<&str>) -> String {
    let mut path = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let close = rest[open..]
            .find('}')
            .expect("a route template closes every parameter it opens")
            + open;
        path.push_str(&rest[..open]);
        let fill = match workspace {
            Some(owned) if &rest[open..=close] == afd_api::route::WORKSPACE_PARAMETER => owned,
            _ => PARAMETER_FILL,
        };
        path.push_str(fill);
        rest = &rest[close + 1..];
    }
    path.push_str(rest);
    path
}
