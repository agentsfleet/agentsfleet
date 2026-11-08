//! A model request crosses the real HTTP boundary once, including redirects.

#![expect(
    clippy::indexing_slicing,
    reason = "the test daemon reads a header the client must have sent"
)]

use super::{RunnerApi, Verb, api, call};
use axum::http::{HeaderMap, StatusCode};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

#[tokio::test]
async fn an_authoring_model_redirect_is_refused_without_following_or_resending() {
    let requests = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&requests);
    let app = axum::Router::new().fallback(move || {
        seen.fetch_add(1, Ordering::SeqCst);
        async { (StatusCode::TEMPORARY_REDIRECT, [("location", "/followed")]) }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let api = api(&format!("http://{address}"), "agt_r_fixture").unwrap();
    let failure = api
        .send(call(Verb::AuthoringModel, "/model-calls", Some(b"{}")))
        .await
        .unwrap_err();
    assert_eq!(failure.refusal_status(), Some(307));
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    server.abort();
}

#[tokio::test]
async fn a_transient_authoring_model_status_has_one_physical_request() {
    let requests = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&requests);
    let app = axum::Router::new().fallback(move |headers: HeaderMap| {
        assert_eq!(headers["authorization"], "Bearer agt_r_fixture");
        seen.fetch_add(1, Ordering::SeqCst);
        async { StatusCode::SERVICE_UNAVAILABLE }
    });
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let api = api(&format!("http://{address}"), "agt_r_fixture").unwrap();
    assert!(
        api.send(call(Verb::AuthoringModel, "/model-calls", Some(b"{}")))
            .await
            .unwrap_err()
            .is_retryable()
    );
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    server.abort();
}
