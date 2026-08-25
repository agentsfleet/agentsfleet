//! Reading a key set over a real socket.
//!
//! The one file here that opens one. It answers itself: a `TcpListener` on a
//! loopback port speaking enough HTTP/1.1 to serve one response, so the cap,
//! the status check and the transport-failure path are exercised against an
//! actual connection rather than a mock of one.
//!
//! No TLS. The property under test is the daemon's own reading — the bound it
//! enforces, the statuses it refuses, what it does when the connection dies —
//! and none of that changes with a cipher underneath it. What TLS would add is
//! a certificate authority fixture and a second failure mode belonging to a
//! layer this crate does not own.
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

mod support;

use afd_auth::verifier::VerifyError;
use afd_identity::HttpKeySet;
use afd_identity::jwks::source::{KeySetSource, MAX_RESPONSE_BYTES};

/// How long a fetch waits before giving up. Generous for a loopback socket and
/// short enough that a hung test fails rather than hangs.
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// What the fake server's request buffer starts at before the read grows it.
const REQUEST_BUFFER_BYTES: usize = 1024;

/// What the one-shot server should do with the connection it accepts.
enum Serve {
    /// Answer with this status and body.
    Response { status: u16, body: Vec<u8> },
    /// Accept the connection and drop it without writing anything.
    Hangup,
}

/// Serves exactly one request on a loopback port, then stops.
///
/// Returns the URL to fetch and the task's handle. Bound to port zero so
/// concurrent tests never collide on a fixed port.
async fn serve_once(what: Serve) -> (String, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("a loopback port");
    let url = format!(
        "http://{}/.well-known/jwks.json",
        listener.local_addr().expect("a bound address")
    );

    let handle = tokio::spawn(async move {
        let Ok((mut socket, _peer)) = listener.accept().await else {
            return;
        };
        match what {
            Serve::Hangup => drop(socket),
            Serve::Response { status, body } => {
                // Drain the request before answering. Closing a socket whose
                // receive buffer still holds unread bytes makes the kernel send
                // RST rather than FIN, and an RST discards whatever the server
                // wrote but the client has not yet taken — so a large body
                // arrives truncated while a small one, already delivered before
                // the close, does not. That is a property of sockets, not of
                // the code under test, and reading first is how a real server
                // avoids it.
                let mut request = Vec::with_capacity(REQUEST_BUFFER_BYTES);
                let mut byte = [0_u8; 1];
                while socket.read_exact(&mut byte).await.is_ok() {
                    request.push(byte[0]);
                    if request.ends_with(b"\r\n\r\n") {
                        break;
                    }
                }

                let head = format!(
                    "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = socket.write_all(head.as_bytes()).await;
                let _ = socket.write_all(&body).await;
                let _ = socket.shutdown().await;
            }
        }
    });

    (url, handle)
}

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a current-thread runtime")
        .block_on(future)
}

/// A served document arrives whole.
#[test]
fn test_a_served_key_set_is_read_back_verbatim() {
    support::install_subscriber();
    block_on(async {
        let document = b"{\"keys\":[]}".to_vec();
        let (url, server) = serve_once(Serve::Response {
            status: 200,
            body: document.clone(),
        })
        .await;

        let fetched = HttpKeySet::new(&url, TIMEOUT)
            .expect("a client")
            .fetch()
            .await
            .expect("the server answered");

        assert_eq!(fetched, document);
        server.await.expect("the server task");
    });
}

/// The endpoint is reported back, for a boot diagnostic to name.
#[test]
fn test_the_fetcher_reports_the_endpoint_it_reads() {
    let source =
        HttpKeySet::new("https://issuer.example/.well-known/jwks.json", TIMEOUT).expect("a client");
    assert_eq!(source.url(), "https://issuer.example/.well-known/jwks.json");
}

/// A body past the cap is refused rather than accumulated.
///
/// `JWKS_MAX_RESPONSE_BYTES` is not a comment: the endpoint is
/// config-controlled rather than trusted, so a response that claims to be a key
/// set and is a hundred megabytes must stop being read.
#[test]
fn test_a_body_past_the_cap_is_refused() {
    support::install_subscriber();
    block_on(async {
        let (url, server) = serve_once(Serve::Response {
            status: 200,
            body: vec![b'x'; MAX_RESPONSE_BYTES + 1],
        })
        .await;

        let refused = HttpKeySet::new(&url, TIMEOUT)
            .expect("a client")
            .fetch()
            .await
            .expect_err("past the cap");

        assert_eq!(refused, VerifyError::KeySetUnavailable);
        server.await.expect("the server task");
    });
}

/// A body exactly at the cap is accepted — the bound is inclusive.
///
/// Worth pinning beside the refusal: an off-by-one here would refuse a
/// legitimate key set that happened to sit on the boundary, and that failure
/// would look like a provider outage rather than a limit.
#[test]
fn test_a_body_exactly_at_the_cap_is_accepted() {
    support::install_subscriber();
    block_on(async {
        let (url, server) = serve_once(Serve::Response {
            status: 200,
            body: vec![b'x'; MAX_RESPONSE_BYTES],
        })
        .await;

        let fetched = HttpKeySet::new(&url, TIMEOUT)
            .expect("a client")
            .fetch()
            .await
            .expect("exactly at the cap is within it");

        assert_eq!(fetched.len(), MAX_RESPONSE_BYTES);
        server.await.expect("the server task");
    });
}

/// A non-success status is refused, whatever body came with it.
///
/// A provider answering 500 with an HTML error page must not have that page
/// parsed as a key set — and a 404 from a mistyped issuer must not be silently
/// treated as "no keys".
#[test]
fn test_a_non_success_status_is_refused() {
    support::install_subscriber();
    block_on(async {
        for status in [301_u16, 400, 401, 404, 429, 500, 503] {
            let (url, server) = serve_once(Serve::Response {
                status,
                body: b"{\"keys\":[]}".to_vec(),
            })
            .await;

            let refused = HttpKeySet::new(&url, TIMEOUT)
                .expect("a client")
                .fetch()
                .await
                .expect_err("not a success");

            assert_eq!(refused, VerifyError::KeySetUnavailable, "status {status}");
            server.await.expect("the server task");
        }
    });
}

/// A connection accepted and then dropped is a transport failure, not a parse.
#[test]
fn test_a_dropped_connection_is_a_transport_failure() {
    support::install_subscriber();
    block_on(async {
        let (url, server) = serve_once(Serve::Hangup).await;

        let refused = HttpKeySet::new(&url, TIMEOUT)
            .expect("a client")
            .fetch()
            .await
            .expect_err("the server hung up");

        assert_eq!(refused, VerifyError::KeySetUnavailable);
        server.await.expect("the server task");
    });
}

/// Nothing listening is the same refusal as anything else the transport does.
///
/// One variant for every transport fault: the caller's only decision is "serve
/// the keys I already hold, or fail", and no distinction among the reasons
/// changes it.
#[test]
fn test_a_refused_connection_is_the_same_refusal() {
    support::install_subscriber();
    block_on(async {
        // Bind and immediately drop, so the port is almost certainly closed.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a loopback port");
        let addr = listener.local_addr().expect("a bound address");
        drop(listener);

        let refused = HttpKeySet::new(
            format!("http://{addr}/.well-known/jwks.json"),
            std::time::Duration::from_millis(500),
        )
        .expect("a client")
        .fetch()
        .await
        .expect_err("nothing is listening");

        assert_eq!(refused, VerifyError::KeySetUnavailable);
    });
}
