//! The supervisor: the trusted half of the runner, outside every sandbox.
//!
//! It speaks the runner verbs through [`afd_wire`] and keeps every duty the
//! daemon relies on — the worker pool, renewal, the report spooled before it is
//! posted and drained until answered, the bounded activity sender, memory
//! hydrate and fenced push, bundle fetch and the capability report
//! (`docs/architecture/runner_fleet.md` §The control protocol). Model keys live
//! only here; no sandbox ever holds one. The boot sweep of what a crashed
//! runner's sandboxes left is the engine's, built on [`StorageHome::sandboxes`].
//!
//! # Shape
//!
//! `client` is the one seam to the daemon, a trait a test fakes with a
//! closure. Each duty is its own module over that seam, and [`run`] composes
//! them: the heartbeat, the spool drain and the worker pool side by side until
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

pub mod capability;
pub mod config;
pub mod error;
pub mod workspace_clone;

mod activity;
mod bundles;
mod client;
mod credentials;
mod drainer;
mod egress;
mod halt;
mod heartbeat;
mod holds;
mod identity;
mod lease_loop;
mod memory;
mod records;
mod renew;
mod report;
mod report_spool;
mod storage_home;
mod turns;
mod verbs;
mod worker_pool;

#[cfg(test)]
mod test_support;
#[cfg(any(test, feature = "test-util"))]
pub mod test_util;

#[cfg(test)]
#[path = "lease_telemetry_tests.rs"]
mod lease_telemetry_tests;

use std::sync::Arc;

use afd_core::clock::{Clock, SystemClock};
use afd_core::env::EnvSource;
use afr_agent::AgentEngine;
use afr_egress::SystemResolver;
use afr_sandbox::{Engine, HostProbe, Limits};
use tokio::sync::{Notify, watch};
use tokio_util::sync::CancellationToken;

pub use self::client::ControlPlane;
pub use self::config::Config;
pub use self::error::{Error, Result};
pub use self::storage_home::StorageHome;

use self::bundles::BundleCache;
use self::client::HttpRunnerApi;
use self::drainer::Drainer;
use self::halt::Halt;
use self::heartbeat::{Assignment, Heartbeat};
use self::holds::Holds;
use self::identity::Whoami;
use self::lease_loop::Lessee;
use self::report_spool::ReportSpool;
use self::workspace_clone::{GITHUB_ORIGIN, Mirrors};

/// Everything a runner process is made of.
#[derive(Debug)]
pub(crate) struct Runner {
    /// The daemon.
    pub(crate) plane: ControlPlane,
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
    Ok(Runner {
        plane: ControlPlane::new(Box::new(HttpRunnerApi::new(config)?)),
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
        spool: ReportSpool::new(&home),
        bundles: BundleCache::new(&home),
        mirrors: Mirrors::new(home.mirrors(), origin),
        limits,
        holds: Holds::start(Arc::clone(&clock)),
        clock,
        halt: Halt::new(shutdown),
        held: Notify::new(),
        whoami: Whoami::default(),
    });
    let (assignment, watching) = watch::channel(Assignment::initial());
    let heartbeat = Heartbeat::new(&lessee.plane, &probe, &lessee.holds)
        .keep_beating(&assignment, &lessee.halt);
    let drainer = Drainer {
        spool: &lessee.spool,
        plane: &lessee.plane,
        halt: &lessee.halt,
        held: &lessee.held,
        holds: &lessee.holds,
    };
    let pool = worker_pool::serve(Arc::clone(&lessee), watching);
    tokio::join!(heartbeat, drainer.run(), pool);
    lessee.holds.shutdown().await;
    if lessee.halt.token_refused() {
        return Err(error::token_refused());
    }
    Ok(())
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
