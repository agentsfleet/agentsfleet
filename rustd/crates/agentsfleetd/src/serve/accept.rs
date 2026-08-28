//! Taking connections off the listener, one task each.
//!
//! Its own file rather than a block in `serve.rs`, and the cut is where the
//! concerns already part: `serve` decides what this process IS — which knobs,
//! which pools, which plane — and this decides what happens to one socket once
//! it arrives. The seam between them is the `Router`, which by this point is
//! built and shared.
//!
//! # The accept syscall is a seam, and the loop must survive it failing
//!
//! `accept()` fails for reasons no test can arrange — a descriptor table that
//! is full, a peer that reset between the SYN and the accept — and a loop that
//! returned on the first one would take the daemon down over a transient the
//! next call would have shrugged off.

use afd_api::connection_builder;
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

/// The accept syscall, as a seam.
///
/// M-MOCKABLE-SYSCALLS. `accept()` fails for reasons a test cannot arrange —
/// the process is out of file descriptors, the peer reset between the SYN and
/// the accept — and the loop's answer to that (log it, keep serving) is the
/// difference between one dropped client and a daemon that stops accepting.
/// Making the syscall a trait is what lets that answer be tested at all.
pub trait Acceptor: Send + 'static {
    /// Waits for the next connection.
    ///
    /// # Errors
    /// Returns whatever the underlying accept returned. A failure is one
    /// client, not the end of serving, and the loop treats it that way.
    fn accept(&self) -> impl Future<Output = std::io::Result<tokio::net::TcpStream>> + Send;
}

impl Acceptor for TcpListener {
    async fn accept(&self) -> std::io::Result<tokio::net::TcpStream> {
        Self::accept(self).await.map(|(stream, _peer)| stream)
    }
}

/// Serves until cancelled, spawning one supervised task per connection.
pub(super) async fn accept_loop<A: Acceptor>(
    listener: A,
    router: axum::Router,
    token: CancellationToken,
) {
    loop {
        let accepted = tokio::select! {
            // Cancellation is checked against a genuinely blocked accept, not
            // between iterations — the property Dimension 7.5 exists to prove.
            () = token.cancelled() => break,
            accepted = listener.accept() => accepted,
        };

        let stream = match accepted {
            Ok(stream) => stream,
            Err(error) => {
                // Hoisted: the `log` bridge duplicates field expressions and
                // llvm-cov scores the copy that never runs.
                let reason = error.to_string();
                tracing::warn!(
                    reason,
                    event = "accept_failed",
                    "accept failed; still serving"
                );
                continue;
            }
        };

        let service = router.clone();
        let connection_token = token.clone();
        tokio::spawn(async move {
            // Bound to a local, not spelled inline: `serve_connection` BORROWS
            // the builder, where the http1-only one consumed a copy of it.
            let builder = connection_builder();
            let served = builder.serve_connection(
                TokioIo::new(stream),
                hyper::service::service_fn(move |request| {
                    let service = service.clone();
                    async move { tower::ServiceExt::oneshot(service, request).await }
                }),
            );
            tokio::select! {
                () = connection_token.cancelled() => {}
                result = served => drop(result),
            }
        });
    }
}

/// Runs `accept_loop` over any [`Acceptor`], for tests that need a faulty one.
///
/// The production path goes through [`boot`], which supplies a real
/// `TcpListener`; this exists so a suite can supply one that fails.
#[cfg(feature = "test-util")]
pub async fn serve_accepts<A: Acceptor>(
    listener: A,
    router: axum::Router,
    token: CancellationToken,
) {
    accept_loop(listener, router, token).await;
}
