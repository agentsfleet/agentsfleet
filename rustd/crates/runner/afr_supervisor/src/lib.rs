//! The supervisor: the trusted half of the runner, outside every sandbox.
//!
//! It speaks the runner verbs through [`af_wire`] and keeps every duty the
//! daemon relies on — the worker pool, renewal, the report retained in memory
//! until acknowledged, the bounded activity sender, memory
//! hydrate and fenced push, native instructions and the capability report
//! (`docs/architecture/runner_fleet.md` §The control protocol). Model keys live
//! only here; no sandbox ever holds one. The boot sweep of what a crashed
//! runner's sandboxes left is the engine's, built on [`StorageHome::sandboxes`].
//!
//! # Shape
//!
//! `client` is the one seam to the daemon, a trait a test fakes with a
//! closure. Each duty is its own module over that seam, and [`run`] composes
//! them: the heartbeat and the worker pool side by side until
//! shutdown. No state is shared behind a lock: the
//! heartbeat publishes its assignment on a watch channel, a coordinator task
//! owns which fleet is busy, each lease is one task that owns its sandbox, and
//! the stop signals are cancellation tokens.
//!
//! # What the binary composes
//!
//! [`boot`], then [`run`] with an engine whose base is
//! [`StorageHome::sandboxes`], so the engine's boot sweep finds what a crashed
//! runner left. [`capability::probe_answer`] is the `probe` command's answer.

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

pub mod capability;
pub mod config;
pub mod error;
pub mod workspace_clone;

mod activity;
mod authoring;
mod client;
mod context;
mod credentials;
mod deadline;
mod downlink;
mod egress;
mod halt;
mod holds;
mod identity;
mod lease_loop;
mod memory;
mod records;
mod recovery;
mod renew;
mod report;
mod storage_home;
mod turns;
mod uplink;
mod verbs;
mod worker_pool;

#[cfg(test)]
mod test_support;
#[cfg(any(test, feature = "test-util"))]
pub mod test_util;

#[cfg(test)]
mod lease_telemetry_tests;

use std::sync::Arc;

use af_core::clock::{Clock, SystemClock};
use af_core::env::EnvSource;
use af_egress::SystemResolver;
use afr_agent::AgentEngine;
use afr_sandbox::{Engine, HostProbe, Limits};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

pub use self::client::ControlPlane;
pub use self::config::Config;
pub use self::error::{Error, Result};
pub use self::storage_home::StorageHome;

use self::client::HttpRunnerApi;
use self::downlink::policy::{Assignment, Heartbeat};
use self::halt::Halt;
use self::holds::Holds;
use self::identity::Whoami;
use self::lease_loop::Lessee;
use self::workspace_clone::{GITHUB_ORIGIN, Mirrors};

/// Everything a runner process is made of.
#[derive(Debug)]
pub(crate) struct Runner {
    /// The daemon.
    pub(crate) plane: ControlPlane,
    pub(crate) downlink: downlink::Downlink,
    pub(crate) offers: tokio::sync::mpsc::Receiver<downlink::Offered>,
    /// The storage root.
    pub(crate) home: StorageHome,
    /// Builds each lease's sandbox.
    pub(crate) engine: Box<dyn Engine>,
    /// Runs each lease's turn.
    pub(crate) agent: Box<dyn AgentEngine>,
    /// What this host's kernel can enforce, probed at boot.
    pub(crate) probe: HostProbe,
    /// What every sandbox enforces.
    pub(crate) limits: Limits,
    /// The wall clock lease deadlines are read against.
    pub(crate) clock: Arc<dyn Clock>,
}

/// Reads the configuration from `env` and opens its storage home.
///
/// The binary boots with it before it builds an engine, so a misinstalled
/// host fails before anything runs.
///
/// # Errors
/// A configuration that is missing or malformed, or a storage root that
/// cannot be made.
pub fn boot(env: &impl EnvSource) -> Result<(Config, StorageHome)> {
    let config = Config::from_env(env)?;
    let home = StorageHome::open(config.storage_home())?;
    Ok((config, home))
}

/// Runs the supervisor against the configured daemon until `shutdown`, or
/// until the daemon says stop.
///
/// # Errors
/// A client that cannot be built, or a daemon that refused this runner's
/// token, which stops it.
pub async fn run(
    config: &Config,
    home: StorageHome,
    engine: Box<dyn Engine>,
    agent: Box<dyn AgentEngine>,
    probe: HostProbe,
    shutdown: CancellationToken,
) -> Result<()> {
    serve(compose(config, home, engine, agent, probe)?, shutdown).await
}

/// [`run`], fetching every bound repository from `origin`, which ends in `/`,
/// rather than from GitHub: the daemon's integration lane serves its fixture
/// repositories over `file://`.
///
/// # Errors
/// As [`run`].
#[cfg(feature = "test-util")]
pub async fn run_fetching_from(
    config: &Config,
    home: StorageHome,
    engine: Box<dyn Engine>,
    agent: Box<dyn AgentEngine>,
    probe: HostProbe,
    origin: &str,
    shutdown: CancellationToken,
) -> Result<()> {
    let runner = compose(config, home, engine, agent, probe)?;
    serve_from(runner, origin, shutdown).await
}

/// The runner a configuration, a storage home and the host's parts make.
fn compose(
    config: &Config,
    home: StorageHome,
    engine: Box<dyn Engine>,
    agent: Box<dyn AgentEngine>,
    probe: HostProbe,
) -> Result<Runner> {
    let (stream, downlink, offers) = downlink::Downlink::new(config)?;
    Ok(Runner {
        plane: ControlPlane::new(Box::new(HttpRunnerApi::new(config)?), stream),
        downlink,
        offers,
        home,
        engine,
        agent,
        probe,
        limits: Limits::default(),
        clock: Arc::new(SystemClock),
    })
}

/// Runs a composed supervisor until `shutdown`, or until the daemon says stop.
pub(crate) async fn serve(runner: Runner, shutdown: CancellationToken) -> Result<()> {
    serve_from(runner, GITHUB_ORIGIN, shutdown).await
}

/// [`serve`], with the bound repositories fetched from `origin`.
async fn serve_from(runner: Runner, origin: &str, shutdown: CancellationToken) -> Result<()> {
    let Runner {
        plane,
        downlink,
        offers,
        home,
        engine,
        agent,
        probe,
        limits,
        clock,
    } = runner;
    let lessee = Arc::new(Lessee {
        plane,
        engine,
        resolver: Box::new(SystemResolver),
        agent,
        mirrors: Mirrors::new(home.mirrors(), origin),
        limits,
        holds: Holds::start(Arc::clone(&clock)),
        clock,
        halt: Halt::new(shutdown),
        whoami: Whoami::default(),
    });
    let (assignment, watching) = watch::channel(Assignment::initial());
    let heartbeat = Heartbeat::new(&lessee.plane, &probe, &lessee.holds)
        .keep_beating(&assignment, &lessee.halt);
    let pool = worker_pool::serve(Arc::clone(&lessee), watching, offers);
    let duties = async {
        tokio::join!(heartbeat, pool);
    };
    tokio::pin!(duties);
    // The connection remains available while accepted workers finish and their reports settle.
    let session_stop = CancellationToken::new();
    let session_halt = Halt::new(session_stop.clone());
    let mut connection = Box::pin(downlink.run(&session_halt));
    let result = tokio::select! {
        result = &mut connection => {
            lessee.halt.stop();
            duties.await;
            result
        }
        () = &mut duties => {
            session_stop.cancel();
            connection.await
        }
    };
    lessee.holds.shutdown().await;
    result?;
    if lessee.halt.token_refused() {
        return Err(error::token_refused());
    }
    Ok(())
}

/// `len` as the wire's byte count; supported hosts have pointers no wider than 64 bits.
pub(crate) fn byte_count(len: usize) -> u64 {
    len as u64
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
