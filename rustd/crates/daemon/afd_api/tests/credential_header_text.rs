//! A credential header that is not visible ASCII, and a native connection's own
//! verdict, are refused in the plane's vocabulary before any credential lookup.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;

use afd_auth::directory::Liveness;
use afd_auth::principal::Runner;
use afd_http::auth::{
    AuthenticatedRunnerStream, ConnectionAuthenticationResult, ConnectionCredentials,
    NativeRunnerConnection, RunnerConnectionAuthentication,
};
use afd_http::handler::Refusal;
use axum::body::Body;
use http::{HeaderMap, Method, Request, StatusCode};
use tower::ServiceExt as _;

use crate::harness::{Fleet, code_of, runner_id, send_with_headers};

/// A runner credential the fixture directory files as live.
const RUNNER_TOKEN: &str = "agt_r0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

#[tokio::test]
async fn a_credential_header_outside_visible_ascii_is_refused_by_each_plane() {
    let fleet = Fleet::new().with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live);
    let directory = fleet.directory().clone();
    let router = fleet.router();
    for (path, code) in [
        ("/v1/api-keys", "UZ-AUTH-002"),
        ("/v1/runners/me", "UZ-RUN-001"),
    ] {
        for header in ["Bearer agt_t\u{e9}", "Bearer \u{1f980}"] {
            let refused = send_with_headers(
                &router,
                Method::GET,
                path,
                None,
                "",
                &[(http::header::AUTHORIZATION, header)],
            )
            .await;
            assert_eq!(refused.status(), StatusCode::UNAUTHORIZED, "{path}");
            assert_eq!(code_of(refused).await, code, "{path}");
        }
    }
    assert_eq!(
        directory.lookups(),
        0,
        "an unreadable header never reaches the credential directory"
    );
}

/// A connection owner that answers every stream with one fixed verdict.
#[derive(Debug)]
struct Verdict(fn() -> ConnectionAuthenticationResult);

impl RunnerConnectionAuthentication for Verdict {
    fn credentials(&self, _headers: &HeaderMap) -> ConnectionAuthenticationResult {
        (self.0)()
    }
}

async fn open(router: &axum::Router, verdict: Verdict) -> axum::response::Response {
    let request = Request::builder()
        .method(Method::POST)
        .uri(af_wire::paths::RUNNER_STREAM)
        .extension(NativeRunnerConnection::new(Arc::new(verdict)))
        .body(Body::from("{}"))
        .expect("the stream opening is well formed");
    router
        .clone()
        .oneshot(request)
        .await
        .expect("axum is infallible")
}

#[tokio::test]
async fn a_native_connection_refusal_is_the_streams_answer_without_a_bearer() {
    let fleet = Fleet::new().with_runner(RUNNER_TOKEN, &runner_id(), Liveness::Live);
    let directory = fleet.directory().clone();
    let router = fleet.router();

    let refused = open(
        &router,
        Verdict(|| {
            Err(Refusal::coded(
                af_core::error_code::RUN_INVALID_RUNNER_TOKEN,
                "The stream names a binding this connection does not hold.",
            ))
        }),
    )
    .await;
    assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(code_of(refused).await, "UZ-RUN-001");

    let bound = open(
        &router,
        Verdict(|| {
            Ok(ConnectionCredentials::Bound {
                runner: Runner::new(runner_id(), false),
                kind: AuthenticatedRunnerStream::new(
                    af_wire::protocol::StreamKind::Reports,
                    runner_id(),
                ),
            })
        }),
    )
    .await;
    assert_eq!(
        bound.status(),
        StatusCode::INTERNAL_SERVER_ERROR,
        "a bound secondary stream is past the guard, at the suite's absent connection owner"
    );
    assert_eq!(code_of(bound).await, "UZ-INTERNAL-003");
    assert_eq!(
        directory.lookups(),
        0,
        "the connection's verdict stands in for the bearer lookup"
    );
}
