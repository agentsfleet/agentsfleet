#![expect(
    clippy::unwrap_used,
    reason = "test target: a fixture that cannot be built is a broken test"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use afd_core::id::Uuid7;
use afd_wire::activity::ActivityRequest;
use afd_wire::credentials::MintCredentialRequest;
use afd_wire::memory::MemoryPushRequest;
use afd_wire::report::{RenewRequest, RenewResponse};
use afd_wire::runner::HeartbeatRequest;
use axum::body::Bytes as AxumBytes;
use axum::http::{HeaderMap, Method, StatusCode, Uri};
use bytes::Bytes;
use tokio::sync::mpsc;

use super::{Call, HttpRunnerApi, RunnerApi, Verb, retrying};
use crate::config::{Config, ENV_API_URL, ENV_RUNNER_TOKEN};
use crate::error;
use crate::test_support::{Answer, FLEET_ID, LEASE_ID, drain, json, plane};

#[tokio::test]
async fn every_verb_goes_to_its_own_path() {
    let (plane, mut calls) = plane(|call| match call.verb {
        Verb::Renew => json(&RenewResponse {
            lease_expires_at: 7,
        }),
        _other => json(&serde_json::json!({})),
    });
    assert!(format!("{plane:?}").contains("FakeApi"));

    send_every_verb(&plane).await;

    let routes: Vec<_> = drain(&mut calls)
        .into_iter()
        .map(|call| (call.verb, call.path.into_owned(), call.body.is_some()))
        .collect();
    let lease_root = format!("/v1/runners/me/leases/{LEASE_ID}");
    let memory = format!("/v1/runners/me/memory/{FLEET_ID}");
    assert_eq!(
        routes,
        vec![
            (
                Verb::Heartbeat,
                "/v1/runners/me/heartbeats".to_owned(),
                true
            ),
            (Verb::Lease, "/v1/runners/me/leases".to_owned(), true),
            (Verb::Renew, format!("{lease_root}/renew"), true),
            (Verb::Activity, format!("{lease_root}/activity"), true),
            (Verb::Report, "/v1/runners/me/reports".to_owned(), true),
            (Verb::Hydrate, memory.clone(), false),
            (Verb::Capture, memory, true),
            (Verb::Bundle, "/v1/runners/me/bundles/ab".to_owned(), false),
            (
                Verb::Mint,
                "/v1/runners/me/credentials/mint".to_owned(),
                true
            ),
            (Verb::Records, format!("{lease_root}/tool-calls"), true),
        ]
    );
}

/// A body whose content no fake daemon reads.
const EMPTY_BODY: &[u8] = b"{}";

/// Sends each verb once, in the order the routes assertion lists them.
async fn send_every_verb(plane: &super::ControlPlane) {
    let lease = Uuid7::parse(LEASE_ID).unwrap();
    let fleet = Uuid7::parse(FLEET_ID).unwrap();
    let heartbeat = HeartbeatRequest {
        capability_report: None,
        selftest: None,
        holds: afd_wire::runner::HeldFleets::default(),
        closing: false,
    };
    let push = MemoryPushRequest {
        lease_id: LEASE_ID.into(),
        fencing_token: 1,
        memory: Vec::new(),
    };
    plane.heartbeat(&heartbeat).await.unwrap();
    plane.lease(&[]).await.unwrap();
    assert_eq!(
        plane.renew(&lease, &RenewRequest::default()).await.unwrap(),
        7,
        "renewal returns its new expiry"
    );
    plane
        .activity(&lease, &ActivityRequest { frames: Vec::new() })
        .await
        .unwrap();
    plane.report(Bytes::from_static(EMPTY_BODY)).await.unwrap();
    plane.hydrate(&fleet).await.unwrap();
    plane.capture(&fleet, &push).await.unwrap();
    plane.bundle("ab").await.unwrap();
    let mint = MintCredentialRequest {
        lease_id: LEASE_ID.into(),
        integration: "github".into(),
        scope: None,
    };
    plane.mint(&mint).await.unwrap();
    plane
        .tool_calls(&lease, Bytes::from_static(EMPTY_BODY))
        .await
        .unwrap();
}

#[tokio::test]
async fn a_reply_decodes_borrowing_or_refuses_as_malformed() {
    let (plane, _calls) = plane(|call| match call.verb {
        Verb::Lease => json(&RenewResponse {
            lease_expires_at: 9,
        }),
        _other => Answer::Reply(Bytes::from_static(b"[")),
    });

    let good = plane.lease(&[]).await.unwrap();
    let bad = plane
        .hydrate(&Uuid7::parse(FLEET_ID).unwrap())
        .await
        .unwrap();

    assert_eq!(good.decode::<RenewResponse>().unwrap().lease_expires_at, 9);
    let refused = bad.decode::<RenewResponse>().unwrap_err();
    assert!(
        refused.to_string().contains("hydrate reply did not decode"),
        "{refused}"
    );
}

#[tokio::test(start_paused = true)]
async fn a_blip_is_retried_and_a_refusal_is_not() {
    let blips = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&blips);
    let recovered = retrying(|| {
        let attempt = counted.fetch_add(1, Ordering::SeqCst);
        async move {
            if attempt < 2 {
                Err(error::unavailable(Verb::Hydrate, 503))
            } else {
                Ok(attempt)
            }
        }
    })
    .await;
    let refusals = Arc::new(AtomicUsize::new(0));
    let refused_count = Arc::clone(&refusals);
    let refused = retrying(|| {
        refused_count.fetch_add(1, Ordering::SeqCst);
        async { Err::<(), _>(error::refused(Verb::Hydrate, 403, None)) }
    })
    .await;
    let exhausted =
        retrying(|| async { Err::<(), _>(error::unavailable(Verb::Report, 502)) }).await;

    assert_eq!(recovered.unwrap(), 2);
    assert_eq!(
        refusals.load(Ordering::SeqCst),
        1,
        "a 4xx is final on its first answer"
    );
    assert!(!refused.unwrap_err().is_retryable());
    assert!(exhausted.unwrap_err().is_retryable());
}

/// What the test daemon saw of one request.
#[derive(Debug)]
struct Seen {
    method: Method,
    path: String,
    authorization: Option<String>,
    content_type: Option<String>,
    body: AxumBytes,
}

/// A daemon that answers `/status/{code}` with that status and a problem body,
/// and everything else with 200 and the body it was sent.
async fn daemon() -> (String, mpsc::UnboundedReceiver<Seen>) {
    let (seen, received) = mpsc::unbounded_channel();
    let app = axum::Router::new().fallback(
        move |method: Method, uri: Uri, headers: HeaderMap, body: AxumBytes| {
            let seen = seen.clone();
            async move {
                let header = |name| {
                    headers
                        .get(name)
                        .map(|value: &axum::http::HeaderValue| value.to_str().unwrap().to_owned())
                };
                let path = uri.path().to_owned();
                let status = path
                    .strip_prefix("/status/")
                    .map_or(StatusCode::OK, |code| {
                        StatusCode::from_u16(code.parse().unwrap()).unwrap()
                    });
                let reply = if status.is_success() {
                    body.clone()
                } else {
                    AxumBytes::from_static(br#"{"error_code":"UZ-RUN-011"}"#)
                };
                seen.send(Seen {
                    method,
                    path,
                    authorization: header("authorization"),
                    content_type: header("content-type"),
                    body,
                })
                .unwrap();
                (status, reply)
            }
        },
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (format!("http://{address}/"), received)
}

fn api(base: &str, token: &str) -> crate::Result<HttpRunnerApi> {
    let env = afd_core::env::MapEnv::from_pairs([(ENV_API_URL, base), (ENV_RUNNER_TOKEN, token)]);
    HttpRunnerApi::new(&Config::from_env(&env)?)
}

fn call(verb: Verb, path: &str, body: Option<&'static [u8]>) -> Call {
    Call {
        verb,
        path: path.to_owned().into(),
        body: body.map(Bytes::from_static),
    }
}

#[tokio::test]
async fn the_http_client_sends_the_token_and_the_right_method() {
    let (base, mut seen) = daemon().await;
    let api = api(&base, "agt_r_token").unwrap();

    let posted = api
        .send(call(Verb::Report, "/v1/report", Some(b"{\"a\":1}")))
        .await
        .unwrap();
    let read = api
        .send(call(Verb::Bundle, "/v1/bundle", None))
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
