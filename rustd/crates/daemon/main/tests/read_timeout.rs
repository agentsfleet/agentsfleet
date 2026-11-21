//! `[server] read_timeout_seconds` bounds what a client sends, never what it
//! is sent.
//!
//! Against the daemon's own accept loop on a real socket, with a one-second
//! timeout. The client is written by hand: the assertions are about a
//! connection closing or staying open, which an HTTP client that pools or
//! retries connections would hide.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::convert::Infallible;
use std::time::{Duration, Instant};

use agentsfleetd::serve::{Drain, serve_accepts};
use axum::body::{Body, Bytes};
use axum::http::header::CONTENT_TYPE;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

/// The timeout under test; short so the suite stays quick.
const READ_TIMEOUT: Duration = Duration::from_secs(1);
/// A ceiling on each wait, so a connection that is never dropped fails the
/// test rather than hanging the lane.
const CEILING: Duration = Duration::from_secs(10);
/// The live stream's events: one every quarter second for three seconds,
/// three times the read timeout.
const TICK: Duration = Duration::from_millis(250);
const TICKS: usize = 12;
const LAST_EVENT: &str = "data: 11\n\n";

/// Answers with the body it was sent, once all of it has arrived.
async fn echo(body: Bytes) -> Bytes {
    body
}

/// A Server-Sent Events (SSE) stream that sends an event every [`TICK`].
async fn live() -> impl IntoResponse {
    let events = futures_util::stream::unfold(0, async |sent: usize| {
        if sent == TICKS {
            return None;
        }
        tokio::time::sleep(TICK).await;
        Some((
            Ok::<_, Infallible>(format!("data: {sent}\n\n")),
            sent.saturating_add(1),
        ))
    });
    (
        [(CONTENT_TYPE, "text/event-stream")],
        Body::from_stream(events),
    )
}

/// The daemon's accept loop on an ephemeral port, with [`READ_TIMEOUT`].
struct Server {
    port: u16,
    abort: CancellationToken,
    serving: JoinHandle<()>,
}

impl Server {
    async fn start() -> Self {
        let router = axum::Router::new()
            .route("/echo", post(echo))
            .route("/live", get(live));
        let listener = TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("an ephemeral port binds");
        let port = listener.local_addr().expect("the bound address").port();
        let abort = CancellationToken::new();
        let serving = tokio::spawn(serve_accepts(
            listener,
            router,
            READ_TIMEOUT,
            Drain::new(),
            abort.clone(),
        ));
        Self {
            port,
            abort,
            serving,
        }
    }

    async fn send(&self, request: &[u8]) -> TcpStream {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port))
            .await
            .expect("the server accepts");
        stream
            .write_all(request)
            .await
            .expect("the request is written");
        stream
    }

    async fn stop(self) {
        self.abort.cancel();
        self.serving.await.expect("the accept loop does not panic");
    }
}

/// Everything the server sent, read until it closed the connection.
async fn until_closed(mut stream: TcpStream) -> String {
    let mut sent = Vec::new();
    let read = tokio::time::timeout(CEILING, stream.read_to_end(&mut sent))
        .await
        .expect("the server closes the connection");
    // A reset is a close too: what matters is that the connection is gone.
    if let Err(error) = read {
        assert_eq!(error.kind(), std::io::ErrorKind::ConnectionReset, "{error}");
    }
    String::from_utf8_lossy(&sent).into_owned()
}

#[tokio::test]
async fn a_request_slower_than_the_read_timeout_is_dropped() {
    let server = Server::start().await;
    let started = Instant::now();
    // A head that never finishes, and a head whose body never does.
    let slow_head = server
        .send(b"POST /echo HTTP/1.1\r\nHost: localhost\r\n")
        .await;
    let slow_body = server
        .send(b"POST /echo HTTP/1.1\r\nHost: localhost\r\nContent-Length: 10\r\n\r\nfirst")
        .await;
    // The control: the same route, sent in full, is answered.
    let prompt = server
        .send(b"POST /echo HTTP/1.1\r\nHost: localhost\r\nContent-Length: 5\r\nConnection: close\r\n\r\nwhole")
        .await;

    let (head_answer, body_answer, prompt_answer) = tokio::join!(
        until_closed(slow_head),
        until_closed(slow_body),
        until_closed(prompt)
    );

    assert!(
        started.elapsed() >= READ_TIMEOUT,
        "the slow requests were given the whole read timeout"
    );
    assert!(
        prompt_answer.starts_with("HTTP/1.1 200") && prompt_answer.ends_with("whole"),
        "a request sent in time is answered: {prompt_answer}"
    );
    assert!(
        !head_answer.starts_with("HTTP/1.1 200"),
        "a head past the timeout is not served: {head_answer}"
    );
    assert!(
        !body_answer.starts_with("HTTP/1.1 200"),
        "a body past the timeout is not served: {body_answer}"
    );
    server.stop().await;
}

#[tokio::test]
async fn a_live_stream_outlives_the_read_timeout() {
    let server = Server::start().await;
    let started = Instant::now();
    let stream = server
        .send(b"GET /live HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await;

    let answer = until_closed(stream).await;

    assert!(
        started.elapsed() >= READ_TIMEOUT.saturating_mul(2),
        "the stream stayed open past the read timeout"
    );
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer}");
    assert!(
        answer.contains(LAST_EVENT),
        "every event arrived, the last one well past the read timeout: {answer}"
    );
    server.stop().await;
}
