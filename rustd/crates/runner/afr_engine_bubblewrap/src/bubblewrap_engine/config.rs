//! What the bubblewrap engine builds sandboxes from.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use af_core::env::{EnvSource, LOG_LEVEL_VAR};
use afr_executor::Sizes;
use afr_image::{HostTools, Toolbox};

use super::SANDBOX_HOST_IDS;
use crate::bubblewrap::SANDBOX_SUBCOMMAND;
use crate::{EngineTuning, ProbePaths};

/// Everything the engine builds sandboxes from.
#[derive(Debug, Clone)]
pub struct BubblewrapConfig {
    /// The host programs it runs.
    pub tools: HostTools,
    /// The mounted, verified toolbox, shared by every clone of the
    /// configuration and unmounted by its one owner.
    pub toolbox: Arc<Toolbox>,
    /// The delegated cgroup each run's cgroup is made under.
    pub cgroup_root: PathBuf,
    /// Where each run's directory is made. Swept when the engine is built.
    pub state_dir: PathBuf,
    /// The binary that hardens and serves inside; `agentsfleet-runner`.
    pub entry: PathBuf,
    /// What it is told; `sandbox`.
    pub entry_args: Vec<OsString>,
    /// The host user and group bubblewrap runs as when the runner is root.
    pub sandbox_ids: (u32, u32),
    /// The log level passed to the process inside, when one is set.
    pub log_level: Option<OsString>,
    /// How long a sandbox may take to answer before the run is refused.
    pub ready_timeout: Duration,
    /// Disk bytes each run's sandbox may read, and write, per second.
    pub io_bytes_per_second: u64,
    /// What each sandbox's executor is started with and its client connects
    /// with.
    pub sizes: Sizes,
}

impl BubblewrapConfig {
    /// What a runner host builds sandboxes from: its verified `toolbox`, the
    /// cgroup delegated to it, `state_dir` for each run's directory, and
    /// `entry`, the runner binary every sandbox starts, paced as `tuning`
    /// says. The log level is the one `env` names.
    #[must_use]
    pub fn for_host(
        toolbox: Arc<Toolbox>,
        (cgroup_root, state_dir): (PathBuf, PathBuf),
        entry: PathBuf,
        tuning: EngineTuning,
        env: &impl EnvSource,
    ) -> Self {
        Self {
            tools: HostTools::default(),
            toolbox,
            cgroup_root,
            state_dir,
            entry,
            entry_args: vec![OsString::from(SANDBOX_SUBCOMMAND)],
            sandbox_ids: SANDBOX_HOST_IDS,
            log_level: env.get(LOG_LEVEL_VAR).map(OsString::from),
            ready_timeout: tuning.ready_timeout,
            io_bytes_per_second: tuning.io_bytes_per_second,
            sizes: tuning.sizes,
        }
    }

    /// Where to probe the host this configuration builds on: its own launcher,
    /// cgroup and state directory, so the probe checks what the engine will use.
    #[must_use]
    pub fn probe_paths(&self) -> ProbePaths {
        ProbePaths {
            bwrap: self.tools.bwrap.clone(),
            cgroup_root: self.cgroup_root.clone(),
            state_dir: Some(self.state_dir.clone()),
            ..ProbePaths::default()
        }
    }
}
