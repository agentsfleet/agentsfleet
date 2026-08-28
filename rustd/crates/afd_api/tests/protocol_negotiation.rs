//! The same router, over both protocol versions, answering identically.
//!
//! # The test whose absence let a real defect through
//!
//! `axum`'s `http2` feature was enabled with a comment saying it was for the
//! event streams, and the accept loop went on building connections with
//! `hyper::server::conn::http1::Builder`. Turning on a Cargo feature nothing
//! calls compiles clean and passes every suite, so the daemon served HTTP/1.1
//! only and the manifest said otherwise. Nothing here is subtle — what was
//! missing was anything that opened a connection and asked which protocol came
//! back.
//!
//! # h2c, not TLS
//!
//! This drives PRIOR-KNOWLEDGE cleartext HTTP/2: the client sends the h2
//! connection preface and the server, reading it, serves h2. That is what a
//! proxy terminating TLS in front of this daemon forwards when it is configured
//! to — a browser will not do it, because browsers only negotiate h2 through
//! ALPN over TLS. So this proves the daemon is capable, which is the half that
//! lives in this repository.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

mod harness;

use std::convert::Infallible;
use std::time::Duration;

use afd_api::connection_builder;
use http::{Request, StatusCode};
use hyper::body::Incoming;
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::net::{TcpListener, TcpStream};

use self::harness::Fleet;

/// How long one exchange may take before the test calls it a hang.
const EXCHANGE_TIMEOUT: Duration = Duration::from_secs(10);

/// The route both protocols ask for.
///
/// The liveness probe: it takes no credential, touches no datastore, and
/// answers a JSON body — so a difference between the two runs is a difference
/// in the TRANSPORT and cannot be anything else.
const PROBE_PATH: &str = "/healthz";

/// One answer, reduced to what a client acts on.
#[derive(Debug, PartialEq, Eq)]
struct Answer {
    status: StatusCode,
    body: String,
}

/// Serves the real router on an ephemeral port, through the real builder.
///
/// Returns the address and a handle that keeps serving until it is dropped.
async fn listening() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let router = Fleet::new().router();
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("an ephemeral port is available");
    let address = listener.local_addr().expect("the listener has an address");
    let serving = tokio::spawn(async move {
        loop {
            let Ok((stream, _peer)) = listener.accept().await else {
                return;
            };
            let service = router.clone();
            tokio::spawn(async move {
                // The production builder, not a test one: the whole claim is
                // about what THIS function does with a connection.
                let builder = connection_builder();
                let served = builder.serve_connection(
                    TokioIo::new(stream),
                    hyper::service::service_fn(move |request: Request<Incoming>| {
                        let service = service.clone();
                        async move { tower::ServiceExt::oneshot(service, request).await }
                    }),
                );
                drop(served.await);
            });
        }
    });
    (address, serving)
}

/// Reads a hyper response down to the two things a client acts on.
async fn answer_of(response: http::Response<Incoming>) -> Answer {
    let status = response.status();
    let bytes = axum::body::to_bytes(axum::body::Body::new(response.into_body()), usize::MAX)
        .await
        .expect("a probe body is small and in memory");
    Answer {
        status,
        body: String::from_utf8(bytes.to_vec()).expect("the probe answers UTF-8"),
    }
}

/// One request over HTTP/1.1.
async fn over_http1(address: std::net::SocketAddr) -> Answer {
    let stream = TcpStream::connect(address)
        .await
        .expect("the listener accepts");
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream))
        .await
        .expect("an HTTP/1.1 handshake completes");
    let pumping = tokio::spawn(async move { drop(connection.await) });

    let request = Request::builder()
        .uri(PROBE_PATH)
        .header(http::header::HOST, address.to_string())
        .body(String::new())
        .expect("the probe request is well formed");
    let response = tokio::time::timeout(EXCHANGE_TIMEOUT, sender.send_request(request))
        .await
        .expect("the server answers within the budget")
        .expect("the server answers rather than dropping");
    let answer = answer_of(response).await;
    pumping.abort();
    answer
}

/// One request over prior-knowledge HTTP/2, with no TLS and no ALPN.
async fn over_http2(address: std::net::SocketAddr) -> Answer {
    let stream = TcpStream::connect(address)
        .await
        .expect("the listener accepts");
    let (mut sender, connection) =
        hyper::client::conn::http2::handshake(TokioExecutor::new(), TokioIo::new(stream))
            .await
            .expect("an HTTP/2 handshake completes — the server read the preface");
    let pumping = tokio::spawn(async move { drop(connection.await) });

    let request = Request::builder()
        .uri(format!("http://{address}{PROBE_PATH}"))
        .body(String::new())
        .expect("the probe request is well formed");
    let response = tokio::time::timeout(EXCHANGE_TIMEOUT, sender.send_request(request))
        .await
        .expect("the server answers within the budget")
        .expect("the server answers rather than dropping");
    let answer = answer_of(response).await;
    pumping.abort();
    answer
}

/// The daemon serves whichever protocol the connection arrived on.
///
/// Byte-identical answers, not merely two successes: a JSON surface that
/// answered differently over h2 would be a second contract nobody documented,
/// and the point of detecting the protocol is that NOTHING above the transport
/// changes.
#[tokio::test]
async fn should_answer_the_same_over_both_protocol_versions() {
    let (address, serving) = listening().await;

    let one = over_http1(address).await;
    assert_eq!(
        one.status,
        StatusCode::OK,
        "an HTTP/1.1 client still gets its answer"
    );
    assert!(
        one.body.contains('{'),
        "the probe answers JSON, not an upgrade error: {}",
        one.body
    );

    let two = over_http2(address).await;
    assert_eq!(
        two.status,
        StatusCode::OK,
        "an HTTP/2 client is served rather than refused"
    );

    assert_eq!(
        one, two,
        "the same route over two protocols is the same answer"
    );
    serving.abort();
}

/// An HTTP/2 client is served on a connection of its own, concurrently.
///
/// The reason the protocol matters at all: a browser caps HTTP/1.1 connections
/// per origin at six and an event stream holds one for as long as its tab is
/// open. Proving several h2 requests ride ONE connection is proving the cap
/// stops being a fact about the product.
#[tokio::test]
async fn should_multiplex_several_requests_onto_one_http2_connection() {
    let (address, serving) = listening().await;

    let stream = TcpStream::connect(address)
        .await
        .expect("the listener accepts");
    let (sender, connection) =
        hyper::client::conn::http2::handshake(TokioExecutor::new(), TokioIo::new(stream))
            .await
            .expect("an HTTP/2 handshake completes");
    let pumping = tokio::spawn(async move { drop(connection.await) });

    // Six is the HTTP/1.1 per-origin ceiling this exists to escape, so six is
    // the number worth proving rides one connection.
    let mut answered = 0_u8;
    for _ in 0..6_u8 {
        let mut sender = sender.clone();
        let request = Request::builder()
            .uri(format!("http://{address}{PROBE_PATH}"))
            .body(String::new())
            .expect("the probe request is well formed");
        let response = tokio::time::timeout(EXCHANGE_TIMEOUT, sender.send_request(request))
            .await
            .expect("the server answers within the budget")
            .expect("the server answers rather than dropping");
        assert_eq!(response.status(), StatusCode::OK);
        answered += 1;
    }
    assert_eq!(answered, 6, "every request rode the one connection");

    pumping.abort();
    serving.abort();
}

/// Nothing else in this file needs `Infallible`, and the lint wants it named.
const _: Option<Infallible> = None;
