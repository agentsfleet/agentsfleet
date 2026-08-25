//! One real connection, driven by hand.
//!
//! hyper enforces the header allowance while PARSING, so there is no request
//! value to hand a `Service` and nothing a tower stack can observe. The bytes
//! have to go onto a socket, which means this helper owns a listener, a hyper
//! connection configured by the code under test, and a client that writes a
//! head it composed itself.
#![expect(
    clippy::expect_used,
    reason = "test support: an unmet precondition should fail the test loudly"
)]

use std::convert::Infallible;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use afd_api::http1_builder;
use http::{Request, Response, StatusCode};
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};

/// What one hand-written request got back.
#[derive(Debug)]
pub(crate) struct Outcome {
    /// The status line's code, or `None` when the connection died first.
    status: Option<u16>,
    /// Whether the service behind the connection was ever invoked.
    handler_ran: bool,
}

impl Outcome {
    /// The status, for a request that was expected to be answered.
    pub(crate) fn expect_status(&self) -> u16 {
        self.status
            .expect("the request was expected to be answered, not dropped")
    }

    /// The status, or `None` when the peer closed instead of answering.
    pub(crate) const fn status(&self) -> Option<u16> {
        self.status
    }

    /// Whether the request reached the service behind the connection.
    pub(crate) const fn handler_ran(&self) -> bool {
        self.handler_ran
    }
}

/// How long any part of one exchange may take before the test gives up.
///
/// A hang is a real failure mode here — a server that neither answers nor
/// closes is exactly what an unbounded read looks like — so it has to fail
/// rather than stall the suite.
const EXCHANGE_TIMEOUT: Duration = Duration::from_secs(10);

/// A well-formed request head padded to roughly `bytes` of headers.
///
/// One long header rather than many, so what is being tested is the SIZE of the
/// head and not hyper's separate limit on how many headers it will index.
///
/// `Connection: close` is not padding: without it hyper holds the connection
/// open after answering, and a client reading to end-of-stream waits for a
/// close that keep-alive is never going to send.
pub(crate) fn request_with_header_bytes(bytes: usize) -> String {
    let padding = "a".repeat(bytes);
    format!(
        "GET /probe HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\
         X-Pad: {padding}\r\n\r\n"
    )
}

/// Serves exactly one connection with the daemon's own builder, writing `head`
/// at it and reporting what came back.
pub(crate) async fn serve_one_request(head: String) -> Outcome {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("an ephemeral port is available");
    let address = listener.local_addr().expect("the listener is bound");
    let handler_ran = Arc::new(AtomicBool::new(false));

    let server = tokio::spawn(accept_one(listener, Arc::clone(&handler_ran)));
    let status = tokio::time::timeout(EXCHANGE_TIMEOUT, exchange(address, head))
        .await
        .expect("the server must answer or close, never hang");
    // The connection is finished either way; the server task's own result is
    // not the assertion — whether the service ran is.
    drop(server.await);

    Outcome {
        status,
        handler_ran: handler_ran.load(Ordering::SeqCst),
    }
}

/// Accepts one connection and serves it under the policy being tested.
async fn accept_one(listener: TcpListener, handler_ran: Arc<AtomicBool>) {
    let Ok((stream, _peer)) = listener.accept().await else {
        return;
    };
    let service = service_fn(move |_request: Request<Incoming>| {
        let handler_ran = Arc::clone(&handler_ran);
        async move {
            handler_ran.store(true, Ordering::SeqCst);
            Ok::<_, Infallible>(Response::new(String::new()))
        }
    });
    // The result is deliberately dropped: an oversize head makes hyper refuse
    // and report a parse error here, which is the SUCCESS path for that case.
    drop(
        http1_builder()
            .serve_connection(TokioIo::new(stream), service)
            .await,
    );
}

/// Writes `head` and reads the status code back, if one arrives.
async fn exchange(address: std::net::SocketAddr, head: String) -> Option<u16> {
    let mut stream = TcpStream::connect(address)
        .await
        .expect("the listener accepts connections");

    // A refused head means the server stops reading mid-write, so this write
    // can legitimately fail with a broken pipe — that is the transport-close
    // outcome, not a test failure. The read is still attempted, because the
    // refusal may already be sitting in the socket.
    drop(stream.write_all(head.as_bytes()).await);
    drop(stream.flush().await);

    let mut response = Vec::new();
    if stream.read_to_end(&mut response).await.is_err() && response.is_empty() {
        return None;
    }
    parse_status(&response)
}

/// The status code from a raw HTTP/1.1 response, if the bytes carry one.
fn parse_status(response: &[u8]) -> Option<u16> {
    let text = String::from_utf8_lossy(response);
    let status_line = text.lines().next()?;
    status_line
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .filter(|code| StatusCode::from_u16(*code).is_ok())
}
