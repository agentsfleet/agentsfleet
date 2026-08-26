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

use afd_api::{Admission, DEFAULT_MAX_IN_FLIGHT, http1_builder};
use afd_core::env::EnvSource;
use afd_db::Db;
use afd_redis::Redis;
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use crate::daemon::{Daemon, Outcome};
#[doc(inline)]
pub use crate::error::BootFailure;
use crate::identity::Capabilities;
use crate::plane::{ServingPlane, Shared};
use crate::preflight::{BootConfig, preflight};
use crate::supervisor::Supervisor;

/// The environment fallback for `--port`, named here and read by [`crate::cli`].
///
/// It is a `clap` `env` fallback rather than something this module reads, so
/// the generated `--help` documents it. Before that it was a second, silent
/// input path — the only one, in fact, since `--port` did not exist.
pub const PORT_KNOB: &str = "PORT";

/// What the daemon binds when neither `--port` nor `PORT` says otherwise.
///
/// `http/server.zig`'s default, kept.
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

/// Opens everything the daemon serves through, in `cmd/serve.zig`'s order.
///
/// # Errors
/// Returns the first stage that could not complete. Environment faults are
/// reported together; a datastore that will not answer is reported alone,
/// because there is nothing to gather once the process cannot proceed.
pub async fn boot<E: EnvSource + ?Sized>(
    env: &E,
    port: u16,
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

    // 3. Everything the router is generic over, chosen here and nowhere else,
    //    then the router over it. The admission ceiling is passed alongside
    //    rather than held by the plane: it is a property of the PROCESS, not a
    //    service a verb acts through, and mixing the two would put a
    //    concurrency limit behind a trait about datastores.
    let (capabilities, sessions) = crate::identity::resolve(config.identity());
    announce_identity(&capabilities);
    // The one clone of the key material, at boot, into the handle every store
    // that opens a sealed row shares. `Kek` zeroes on drop, so the copy this
    // makes is not a copy that outlives the process.
    let kek = Arc::new(config.kek().clone());
    // The broker is built BEFORE the plane because it reads this deployment's
    // own platform credentials out of the vault, which is an asynchronous step
    // the plane's constructor is not. A deployment holding none still boots and
    // still serves every other verb.
    let broker = crate::credentials::resolve(
        &afd_fleet::vault::Vault::new(database.clone(), Arc::clone(&kek)),
        config.platform_admin_workspace(),
    )
    .await;
    let plane: Shared = Arc::new(ServingPlane::new(crate::plane::PlaneParts {
        database: database.clone(),
        queue: queue.clone(),
        kek,
        capabilities,
        sessions,
        bundles: crate::bundles::resolve(config.bundles()),
        broker,
        login: crate::plane::LoginConfig {
            code_pepper: config.session_code_pepper().clone(),
            app_url: config.app_url().to_owned(),
        },
    }));
    let router = afd_api::router::build(plane, &Admission::new(DEFAULT_MAX_IN_FLIGHT));

    // 4. The background sweepers, before the listener: they read through pools
    //    that are open by now, and starting them after the socket would leave a
    //    window where the plane serves while nothing is noticing dead runners.
    crate::sweepers::spawn(&mut *supervisor, &database, &queue);

    // 5. Listen last. Until this line the process is not reachable, which is
    //    what makes every refusal above a refusal rather than an outage.
    let listener = TcpListener::bind((std::net::Ipv4Addr::UNSPECIFIED, port)).await?;
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

/// Says which surfaces this instance can actually serve.
///
/// Once, at boot, because the alternative is an operator discovering it from a
/// 503 on their first enrolment. Reads the RESOLVED seam rather than the config
/// it was built from: preflight has already refused a boot whose provider knobs
/// were missing, so the only way to reach the warning below is a provider that
/// was configured and could not be constructed — which is a reduced surface,
/// not a fault, because the runner plane consults neither seam.
fn announce_identity(capabilities: &Capabilities) {
    match capabilities {
        Capabilities::Provider(_built) => {
            tracing::info!(
                event = "identity_provider_configured",
                "identity provider configured — tenant and runner planes both serve"
            );
        }
        Capabilities::Unconfigured(_absent) => {
            // Hoisted: the `log` bridge duplicates field expressions and
            // llvm-cov scores the dead copy.
            let code = afd_core::error_code::AUTH_UNAVAILABLE.as_str();
            tracing::warn!(
                error_code = code,
                event = "identity_provider_unusable",
                "identity provider unusable — the runner plane serves normally \
                 and every tenant-plane capability read answers unavailable"
            );
        }
    }
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
/// `port` is already resolved — [`crate::cli`] took it from `--port`, the
/// `PORT` fallback, or [`DEFAULT_PORT`], and rejected anything unusable before
/// this was called. Nothing here re-reads the environment for it.
///
/// # Errors
/// Returns the boot stage that could not complete: an environment naming every
/// fault at once, a datastore that would not answer, or a port that would not
/// bind. Serving never starts in any of those cases.
pub async fn run<E: EnvSource + ?Sized, F>(
    env: &E,
    port: u16,
    signal: F,
) -> Result<Outcome, BootFailure>
where
    F: Future<Output = ()>,
{
    let mut supervisor = Supervisor::new();
    let booted = boot(env, port, &mut supervisor).await?;
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
