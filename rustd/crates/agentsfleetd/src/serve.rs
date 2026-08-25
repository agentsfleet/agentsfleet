//! Boot: what happens between a resolved environment and a served port.
//!
//! The order is `cmd/serve.zig`'s, because the order is the part that carries
//! meaning — pools before Redis before the router, and nothing listening until
//! all of them answered. What is NOT ported is the shape: `serve.zig` is one
//! function holding a defer chain that teardown has to unwind in exactly the
//! right sequence, and the sequence is only correct because the declarations
//! happen to be in the right order. Here the ordering lives in types:
//! [`Supervisor::shutdown`] consumes itself, so nothing borrowed can be dropped
//! early, and [`Daemon::run`] tears down on every path out.
//!
//! # Why hyper directly rather than `axum::serve`
//!
//! `axum::serve` exposes no way to bound the header buffer, and Dimension 5.3
//! is a bound on the header buffer. So the accept loop is ours and
//! [`afd_api::http1_builder`] carries the limit — the same builder §5's
//! oversize-head test drives.
//!
//! # Every connection is a supervised child of the accept loop
//!
//! A connection task is spawned per accepted socket and given the same
//! cancellation token. `serve.zig` has no equivalent because httpz owns its own
//! worker pool; here the alternative would be `tokio::spawn`, which detaches —
//! and a detached connection can outlive the pools it reads through. That is
//! the unsupervised spawn path Dimension 7.5 says does not exist.

use std::net::SocketAddr;
use std::sync::Arc;

use afd_api::http1_builder;
use afd_core::env::EnvSource;
use afd_db::Db;
use afd_redis::Redis;
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::daemon::{Daemon, Outcome};
use crate::preflight::{BootConfig, Refusal, preflight};
use crate::probes::LiveDependencies;
use crate::supervisor::Supervisor;

/// The port knob, and the default `http/server.zig` listens on.
pub const PORT_KNOB: &str = "PORT";

/// What the daemon binds when `PORT` is unset.
pub const DEFAULT_PORT: u16 = 3000;

/// Everything boot opened, in the order it opened it.
///
/// Returned rather than kept, so the caller holds the pools and drops them
/// AFTER the shutdown that joined every task reading through them.
#[derive(Debug)]
pub struct Booted {
    /// The address actually bound — resolved, so port 0 reports what it got.
    pub address: SocketAddr,
    /// The Postgres pool, to be dropped only after shutdown returns.
    pub database: Db,
    /// The Redis client, likewise.
    pub queue: Redis,
}

/// Why boot could not finish.
///
/// A flat crate-level enum composed by `From`, which is the shape both
/// reference implementations settled on — bun's `libarchive::Error`
/// (`#[error(transparent)] #[from]` per foreign error, `?` does the lifting)
/// and habitat's `sup::Error` (payload-carrying variants, one `Result` alias).
///
/// Every variant carries the ORIGINAL error as a `#[source]`, never a string.
/// An earlier revision of this type stringified them, which compiled, read
/// fine, and quietly defeated [`crate::fatal`] — the renderer walks
/// `source()` to print the causal chain, and there was nothing left to walk.
/// A `to_string()` on the way into an error type is a lossy conversion wearing
/// a conversion's clothes.
#[derive(Debug, thiserror::Error)]
pub enum BootFailure {
    /// The environment is unusable; every fault named at once.
    #[error(transparent)]
    Environment(#[from] Refusal),
    /// Postgres would not answer.
    #[error("agentsfleetd cannot boot: the API database would not answer")]
    Database(#[from] afd_db::Error),
    /// Redis would not answer.
    #[error("agentsfleetd cannot boot: the API queue would not answer")]
    Queue(#[from] afd_redis::Error),
    /// The port could not be bound.
    #[error("agentsfleetd cannot listen")]
    Listen(#[from] std::io::Error),
}

/// Reads the port from `env`, falling back to the Zig daemon's default.
///
/// An unparseable value is the default rather than a refusal, matching
/// `serve.zig`, whose `--port` parse failure is the only port error it raises.
///
/// Public because "what port would this boot bind?" is a question worth being
/// able to ask without booting, and because a rule this quiet — a bad value is
/// silently the default — should be assertable rather than discovered.
#[must_use]
pub fn port_from<E: EnvSource + ?Sized>(env: &E) -> u16 {
    env.get(PORT_KNOB)
        .and_then(|raw| raw.trim().parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

/// Opens everything the daemon serves through, in `cmd/serve.zig`'s order.
///
/// # Errors
/// Returns the first stage that could not complete. Environment faults are
/// reported together; a datastore that will not answer is reported alone,
/// because there is nothing to gather once the process cannot proceed.
pub async fn boot<E: EnvSource + ?Sized>(
    env: &E,
    supervisor: &mut Supervisor,
) -> Result<Booted, BootFailure> {
    // 1. Every knob, validated before a single socket opens. A key that cannot
    //    work must refuse boot BEFORE anything serves, which is only a promise
    //    if nothing has been opened yet.
    let config: BootConfig = preflight(env)?;

    // 2. Postgres, then Redis — the Zig order, and the useful one: a daemon
    //    with no database has nothing to serve, so it is the first thing worth
    //    failing on.
    let database = Db::connect(config.api_pool()).await?;
    let queue = Redis::connect(config.redis()).await?;

    // 3. The router, over the probe that reads through both.
    let dependencies = Arc::new(LiveDependencies::new(database.clone(), queue.clone()));
    let router = afd_api::router::build(dependencies);

    // 4. Listen last. Until this line the process is not reachable, which is
    //    what makes every refusal above a refusal rather than an outage.
    let listener = TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, port_from(env))).await?;
    let address = listener.local_addr()?;

    supervisor.spawn(ACCEPT_LOOP, move |token| {
        accept_loop(listener, router, token)
    });

    Ok(Booted {
        address,
        database,
        queue,
    })
}

/// The supervised name of the accept loop.
pub const ACCEPT_LOOP: &str = "accept_loop";

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
async fn accept_loop<A: Acceptor>(listener: A, router: axum::Router, token: CancellationToken) {
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
                tracing::warn!(reason, "accept failed; still serving");
                continue;
            }
        };

        let service = router.clone();
        let connection_token = token.clone();
        tokio::spawn(async move {
            let served = http1_builder().serve_connection(
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

/// Runs the daemon to completion, then drops what it borrowed.
///
/// The last line is the point: `booted` — the pools — is dropped only after
/// `Daemon::run` has cancelled and JOINED every task, which is invariant C2
/// expressed as the order of two statements the compiler will not let you swap.
///
/// # Errors
/// Returns the boot stage that could not complete: an environment naming every
/// fault at once, a datastore that would not answer, or a port that would not
/// bind. Serving never starts in any of those cases.
pub async fn run<E: EnvSource + ?Sized, F>(env: &E, signal: F) -> Result<Outcome, BootFailure>
where
    F: Future<Output = ()>,
{
    let mut supervisor = Supervisor::new();
    let booted = boot(env, &mut supervisor).await?;
    crate::banner::show(
        env!("CARGO_PKG_VERSION"),
        &[
            format!("listening {}", booted.address),
            format!("postgres:{}", booted.database.role().tag()),
            format!("redis:{}", booted.queue.role().tag()),
        ],
        std::process::id(),
    );

    // The server future is `pending`: serving happens in the supervised accept
    // loop, so what ends this run is the signal — or the accept loop itself
    // dying, which `Daemon` would then have to report. Modelled as pending
    // because a loop that returned would leave the supervisor with a finished
    // task rather than a stopped server, and `Daemon::run` reads the outcome
    // from the report either way.
    let outcome = Daemon::new(supervisor)
        .run(std::future::pending(), signal)
        .await;

    // C2, as two statements the compiler will not let you swap: the pools are
    // dropped only after a shutdown that joined every task reading through them.
    drop(booted);
    Ok(outcome)
}
