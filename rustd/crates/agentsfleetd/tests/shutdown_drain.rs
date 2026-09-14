//! Dimension 7.7 — a deployment replaces processes, and no request with it.
//!
//! Against a real listener on a real socket, because the property is about what
//! a client on the other end of a TCP connection observes, and a fake acceptor
//! cannot observe a closed port or a truncated response. The client is written
//! by hand rather than through an HTTP crate: the assertion is about bytes on
//! the wire — a complete status line and a complete body — and a client that
//! retries or pools connections would hide exactly the failure being tested.
//!
//! The shape under test is the split between two tokens. Before Dimension 7.7
//! both halves of a stop were one token, so a signal did not stop the server, it
//! cut it: a request halfway through had its future dropped mid-await and the
//! caller saw a closed socket with no status.

#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use agentsfleetd::serve::{Drain, serve_accepts};
use axum::extract::State;
use axum::routing::get;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

/// The body the slow handler answers with. Asserted in full, so a response
/// truncated by a cut connection fails rather than passing on its prefix.
const SLOW_BODY: &str = "finished-after-the-signal";

/// Generous enough that a loaded machine cannot fail a drain that is working.
const GENEROUS_BOUND: Duration = Duration::from_secs(10);
/// A ceiling on the whole test, so a hang is a failure rather than a hung lane.
const TEST_CEILING: Duration = Duration::from_secs(20);

/// What the handler and the test share: a count the test spins on, and the
/// release the test fires once it has seen the request arrive.
#[derive(Clone)]
struct Gate {
    entered: Arc<AtomicUsize>,
    release: Arc<Notify>,
}

/// A request that is inside the server when the signal lands.
async fn slow(State(gate): State<Gate>) -> &'static str {
    gate.entered.fetch_add(1, Ordering::SeqCst);
    // `notify_one` stores a permit, so a release that fires before this line is
    // reached still completes it — the wake cannot be lost in the window
    // between the count above and the await here.
    gate.release.notified().await;
    SLOW_BODY
}

/// True once nothing is listening on `port` any more.
async fn refuses_new_connections(port: u16) -> bool {
    for _ in 0..REFUSAL_ATTEMPTS {
        match TcpStream::connect(("127.0.0.1", port)).await {
            Err(_) => return true,
            Ok(stream) => drop(stream),
        }
        tokio::time::sleep(REFUSAL_POLL).await;
    }
    false
}

const REFUSAL_ATTEMPTS: u32 = 200;
const REFUSAL_POLL: Duration = Duration::from_millis(10);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_shutdown_drains_in_flight_requests() {
    tokio::time::timeout(TEST_CEILING, drains_in_flight_requests())
        .await
        .expect("the drain finishes well inside the test ceiling");
}

async fn drains_in_flight_requests() {
    let gate = Gate {
        entered: Arc::new(AtomicUsize::new(0)),
        release: Arc::new(Notify::new()),
    };
    let router = axum::Router::new()
        .route("/slow", get(slow))
        .with_state(gate.clone());

    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("an ephemeral port binds");
    let port = listener.local_addr().expect("the bound address").port();

    let drain = Drain::new();
    let abort = CancellationToken::new();
    let serving = tokio::spawn(serve_accepts(
        listener,
        router,
        drain.clone(),
        abort.clone(),
    ));

    // ── A request is in flight when the signal lands.
    let mut client = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("the daemon is accepting before the drain");
    client
        .write_all(b"GET /slow HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .expect("the request is written");
    while gate.entered.load(Ordering::SeqCst) == 0 {
        tokio::task::yield_now().await;
    }

    // ── The signal: stop accepting, and wait for what is in flight.
    let draining = tokio::spawn({
        let drain = drain.clone();
        async move { drain.settle(GENEROUS_BOUND).await }
    });

    // ── A NEW connection is refused, by the kernel, because the accept loop
    //    broke and dropped the listener.
    assert!(
        refuses_new_connections(port).await,
        "the port must stop answering once the drain starts"
    );

    // ── The in-flight request is still alive, so it finishes.
    gate.release.notify_one();
    let settled = draining.await.expect("the drain task does not panic");
    assert_eq!(
        settled.in_flight_at_close, 1,
        "one request was in flight when accepting stopped"
    );
    assert_eq!(settled.abandoned, 0, "nothing was left to cut");
    assert!(settled.is_clean());

    // ── And its caller got a WHOLE answer: a status line and the full body.
    //    This is the assertion the old single-token shutdown could not pass.
    let mut answer = String::new();
    client
        .read_to_string(&mut answer)
        .await
        .expect("the response is readable");
    assert!(
        answer.starts_with("HTTP/1.1 200 OK"),
        "the in-flight request completed with a status: {answer:?}"
    );
    assert!(
        answer.ends_with(SLOW_BODY),
        "the body arrived in full, not truncated by a cut: {answer:?}"
    );

    // ── Only now is anything cancelled, and the loop is already gone.
    abort.cancel();
    serving.await.expect("the accept loop ended cleanly");
}

/// The other half of the bound: a request that never finishes must not hold a
/// deployment open. The drain gives up, says how many it left, and the caller
/// carries on to the supervisor that cuts them.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_shutdown_stops_within_its_bound_when_a_request_will_not_finish() {
    let gate = Gate {
        entered: Arc::new(AtomicUsize::new(0)),
        release: Arc::new(Notify::new()),
    };
    let router = axum::Router::new()
        .route("/slow", get(slow))
        .with_state(gate.clone());

    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("an ephemeral port binds");
    let port = listener.local_addr().expect("the bound address").port();

    let drain = Drain::new();
    let abort = CancellationToken::new();
    let serving = tokio::spawn(serve_accepts(listener, router, drain.clone(), abort.clone()));

    let mut client = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("the daemon is accepting");
    client
        .write_all(b"GET /slow HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .expect("the request is written");
    while gate.entered.load(Ordering::SeqCst) == 0 {
        tokio::task::yield_now().await;
    }

    // The request is never released. A short bound stands in for the shipped
    // one, so the test spends milliseconds proving what the deployment would
    // spend DRAIN_TIMEOUT proving.
    let settled = drain.settle(Duration::from_millis(100)).await;
    assert_eq!(settled.in_flight_at_close, 1);
    assert_eq!(settled.abandoned, 1, "the unfinished request is reported");
    assert!(
        !settled.is_clean(),
        "an expired bound is not a clean stop, and the run must say so"
    );

    // The supervisor's token is what actually cuts it — the step the drain
    // deliberately runs BEFORE.
    abort.cancel();
    gate.release.notify_one();
    serving.await.expect("the accept loop ended");
}
