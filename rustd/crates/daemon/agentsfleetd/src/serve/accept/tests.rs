//! The accept loop's two ways out, each against an accept that never returns.

#![expect(
    clippy::expect_used,
    reason = "a joined test task's panic should surface here, not be swallowed"
)]

use std::time::Duration;

use super::*;

/// An acceptor whose accept never returns: the one place the loop blocks.
struct Parked;

impl Acceptor for Parked {
    async fn accept(&self) -> std::io::Result<tokio::net::TcpStream> {
        std::future::pending().await
    }
}

/// Long enough to join a loop that left; a tenth of the supervisor's join
/// timeout, which is what a loop that ignored its token used to cost.
const PROMPT: Duration = Duration::from_secs(1);

/// Starts the loop and lets it reach its blocked accept.
async fn blocked(drain: &Drain, abort: &CancellationToken) -> tokio::task::JoinHandle<()> {
    let serving = tokio::spawn(accept_loop(
        Parked,
        axum::Router::new(),
        drain.clone(),
        abort.clone(),
    ));
    tokio::task::yield_now().await;
    assert!(!serving.is_finished(), "nothing has asked the loop to stop");
    serving
}

/// A supervisor cancel that skipped the drain stops a blocked accept, and the
/// loop still reports its listener dropped.
#[tokio::test]
async fn the_supervisor_token_alone_stops_a_blocked_accept() {
    let (drain, abort) = (Drain::new(), CancellationToken::new());
    let serving = blocked(&drain, &abort).await;

    abort.cancel();
    tokio::time::timeout(PROMPT, serving)
        .await
        .expect("the loop leaves on the supervisor's token")
        .expect("the loop does not panic");
    assert!(drain.stopped().is_cancelled(), "a loop that left says so");
    assert!(
        !drain.accepting().is_cancelled(),
        "the drain was never asked"
    );
}

/// The graceful path is unchanged: the drain's token stops the loop while the
/// supervisor's, which cuts connections in flight, is still untouched.
#[tokio::test]
async fn the_drain_token_stops_it_without_the_supervisor() {
    let (drain, abort) = (Drain::new(), CancellationToken::new());
    let serving = blocked(&drain, &abort).await;

    drain.accepting().cancel();
    tokio::time::timeout(PROMPT, serving)
        .await
        .expect("the loop leaves on the drain's token")
        .expect("the loop does not panic");
    assert!(drain.stopped().is_cancelled(), "a loop that left says so");
    assert!(!abort.is_cancelled(), "connections in flight were not cut");
}
