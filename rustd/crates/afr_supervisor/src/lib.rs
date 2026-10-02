//! The supervisor: the trusted half of the runner, outside every sandbox.
//!
//! It speaks the runner verbs through [`afd_wire`] and keeps every duty the
//! daemon relies on — the worker pool, renewal, the report spooled before it is
//! posted, the bounded activity sender, credential minting, memory hydrate and
//! fenced push, bundle fetch, the boot sweep and the capability report
//! (`docs/architecture/runner_fleet.md` §The control protocol). Model keys live
//! only here; no sandbox ever holds one.
//!
//! # Shape
//!
//! [`client`] is the one seam to the daemon, a trait a test fakes with a
//! closure. Each duty is its own module over that seam, and [`serve`] composes
//! them: boot sweep, spool replay, a first heartbeat that proves the token,
//! then the heartbeat and the worker pool side by side until shutdown. No state
//! is shared behind a lock: the heartbeat publishes its assignment on a watch
//! channel, a coordinator task owns which fleet is busy, and each lease is one
//! task that owns its sandbox.

pub mod activity;
pub mod bundles;
pub mod capability;
pub mod client;
pub mod config;
pub mod credentials;
pub mod error;
pub mod heartbeat;
pub mod lease_loop;
pub mod memory;
pub mod renew;
pub mod report;
pub mod report_spool;
pub mod storage_home;
pub mod turns;
pub mod worker_pool;

#[cfg(test)]
mod test_support;

use std::sync::Arc;

use afd_wire::runner::HeartbeatStatus;
use afr_agent::AgentEngine;
use afr_sandbox::{Engine, HostProbe, Limits};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

pub use self::client::{ControlPlane, HttpRunnerApi, RunnerApi};
pub use self::config::Config;
pub use self::error::{Error, Result};

use self::bundles::BundleCache;
use self::heartbeat::Heartbeat;
use self::lease_loop::Lessee;
use self::report_spool::ReportSpool;
use self::storage_home::StorageHome;

const EVENT_SWEPT: &str = "storage_home_swept";
const EVENT_REPLAYED: &str = "report_spool_replay_answered";
const EVENT_REPLAY_FAILED: &str = "report_spool_replay_failed";

/// Everything a runner process is made of, composed by the binary.
#[derive(Debug)]
pub struct Runner {
    /// The daemon.
    pub plane: ControlPlane,
    /// The storage root.
    pub home: StorageHome,
    /// Builds each lease's sandbox.
    pub engine: Box<dyn Engine>,
    /// Runs each lease's turn.
    pub agent: Box<dyn AgentEngine>,
    /// What this host's kernel can enforce, probed at boot.
    pub probe: HostProbe,
    /// What every sandbox enforces.
    pub limits: Limits,
}

/// Runs the supervisor against the configured daemon until `shutdown`.
///
/// # Errors
/// An unusable configuration, a storage root that cannot be made, or a first
/// heartbeat the daemon refuses.
pub async fn run(
    config: &Config,
    engine: Box<dyn Engine>,
    agent: Box<dyn AgentEngine>,
    probe: HostProbe,
    shutdown: CancellationToken,
) -> Result<()> {
    let runner = Runner {
        plane: ControlPlane::new(Box::new(HttpRunnerApi::new(config)?)),
        home: StorageHome::open(config.storage_home())?,
        engine,
        agent,
        probe,
        limits: Limits::default(),
    };
    serve(runner, shutdown).await
}

/// Runs a composed supervisor until `shutdown`, or until the daemon says stop.
///
/// # Errors
/// A boot sweep that cannot run, a spool that cannot be read, or a first
/// heartbeat that fails: a runner the daemon will not answer serves nothing.
pub async fn serve(runner: Runner, shutdown: CancellationToken) -> Result<()> {
    let Runner {
        plane,
        home,
        engine,
        agent,
        probe,
        limits,
    } = runner;
    let swept = home.sweep()?;
    let event = EVENT_SWEPT;
    tracing::info!(swept, event);
    let spool = ReportSpool::new(&home);
    replay(&spool, &plane).await?;
    let lessee = Arc::new(Lessee {
        plane,
        engine,
        agent,
        spool,
        bundles: BundleCache::new(&home),
        limits,
    });
    let mut heartbeat = Heartbeat::new(&lessee.plane, &probe);
    let first = heartbeat.beat().await?;
    if first.status == HeartbeatStatus::Stop {
        return Ok(());
    }
    let (assignment, watching) = watch::channel(first);
    let pool = worker_pool::serve(Arc::clone(&lessee), watching, shutdown.clone());
    tokio::join!(heartbeat.keep_beating(&assignment, &shutdown), pool);
    Ok(())
}

/// Posts every report a previous process spooled, once each.
async fn replay(spool: &ReportSpool, plane: &ControlPlane) -> Result<()> {
    for spooled in spool.pending()? {
        match spooled.deliver(plane).await {
            Ok(delivery) => {
                let event = EVENT_REPLAYED;
                let accepted = delivery == report_spool::Delivery::Accepted;
                tracing::info!(accepted, event);
            }
            Err(failure) => {
                let code = failure.code().as_str();
                let event = EVENT_REPLAY_FAILED;
                tracing::warn!(
                    error_code = code,
                    event,
                    "a spooled report waits for the next boot"
                );
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
