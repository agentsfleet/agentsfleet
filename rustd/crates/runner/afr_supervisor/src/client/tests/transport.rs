//! The real HTTP client keeps bearer scope, status classes and base paths.
use super::{Method, RunnerApi, Verb, api, call, daemon};

#[tokio::test]
async fn the_http_client_sends_the_token_and_the_right_method() {
    let (base, mut seen) = daemon().await;
    let api = api(&base, "agt_r_token").unwrap();

    let posted = api
        .send(call(Verb::Report, "/v1/report", Some(b"{\"a\":1}")))
        .await
        .unwrap();
    let read = api
        .send(call(Verb::Hydrate, "/v1/memory", None))
        .await
        .unwrap();
    api.send(call(Verb::Lease, "/v1/lease", None))
        .await
        .unwrap();

    let first = seen.recv().await.unwrap();
    let second = seen.recv().await.unwrap();
    let third = seen.recv().await.unwrap();
    assert_eq!(&*posted, b"{\"a\":1}");
    assert!(read.is_empty());
    assert_eq!(
        (first.method, first.path.as_str()),
        (Method::POST, "/v1/report")
    );
    assert_eq!(first.authorization.as_deref(), Some("Bearer agt_r_token"));
    assert_eq!(first.content_type.as_deref(), Some("application/json"));
    assert_eq!(&*first.body, b"{\"a\":1}");
    assert_eq!(second.method, Method::GET);
    assert_eq!(second.content_type, None);
    assert_eq!(third.method, Method::POST);
}

#[tokio::test]
async fn the_http_client_classifies_every_status_class() {
    let (base, _seen) = daemon().await;
    let api = api(&base, "agt_r_token").unwrap();

    let busy = api
        .send(call(Verb::Lease, "/status/503", None))
        .await
        .unwrap_err();
    let throttled = api
        .send(call(Verb::Lease, "/status/429", None))
        .await
        .unwrap_err();
    let lost = api
        .send(call(Verb::Renew, "/status/409", Some(b"{}")))
        .await
        .unwrap_err();
    let unauthorized = api
        .send(call(Verb::Heartbeat, "/status/401", Some(b"{}")))
        .await
        .unwrap_err();

    assert!(busy.is_retryable() && throttled.is_retryable());
    assert_eq!(
        lost.refusal_code(),
        Some(afd_core::error_code::RUN_LEASE_LOST)
    );
    assert!(!lost.is_retryable());
    assert!(unauthorized.is_unauthorized());
}

#[tokio::test]
async fn an_unreachable_daemon_is_a_transport_failure() {
    let api = api("http://127.0.0.1:1", "agt_r_token").unwrap();

    let failure = api
        .send(call(Verb::Heartbeat, "/x", None))
        .await
        .unwrap_err();

    assert!(failure.is_retryable());
}

#[test]
fn a_token_no_header_can_carry_is_refused_before_any_call() {
    let refused = api("http://127.0.0.1:1", "agt_r\nbad").unwrap_err();

    assert!(
        refused.to_string().contains("a header cannot carry"),
        "{refused}"
    );
}

#[tokio::test]
async fn a_route_joins_under_the_daemons_path_prefix() {
    let (base, mut seen) = daemon().await;
    let api = api(&format!("{base}gateway"), "agt_r_token").unwrap();

    api.send(call(Verb::Lease, "/v1/runners/me/leases", None))
        .await
        .unwrap();

    assert_eq!(
        seen.recv().await.unwrap().path,
        "/gateway/v1/runners/me/leases"
    );
}
