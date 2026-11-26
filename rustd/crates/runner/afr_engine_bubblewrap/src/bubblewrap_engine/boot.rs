//! Bubblewrap's startup requirements, toolbox verification and resource lifetime.

use std::path::PathBuf;

use af_core::env::EnvSource;
use af_core::error_code::{Coded as _, Logged};
use afr_engine::error::refused;
use afr_engine::{Backend, HostPaths, HostProbe, Resources, Result, Started};
use afr_image::{KernelMounter, MountedToolboxes, Release, ToolboxHome, Toolboxes};

use super::{BubblewrapConfig, BubblewrapEngine, EVENT_HOST_REFUSED};

/// The release and entry point used by the shipping sandbox backend.
#[derive(Debug)]
pub struct BubblewrapBackend<'a, E: EnvSource> {
    /// Binary started inside each sandbox.
    pub entry: PathBuf,
    /// Identity that verifies the staged toolbox.
    pub release: &'a Release,
    /// Images and mount locations supplied by the storage home.
    pub home: &'a ToolboxHome,
    /// Host settings the backend passes into its sandbox.
    pub env: &'a E,
    /// How every sandbox is started and paced.
    pub tuning: crate::EngineTuning,
}

impl<E: EnvSource> Backend for BubblewrapBackend<'_, E> {
    type Engine = BubblewrapEngine;

    /// Enables the delegated controllers and probes the host there.
    fn measure(&self, paths: &HostPaths) -> Result<HostProbe> {
        crate::measure(paths)
    }

    fn validate(&self, host: &HostProbe) -> Result<()> {
        admissible(host)
    }

    fn start(self, paths: &HostPaths, host: &HostProbe) -> Result<Started<Self::Engine>> {
        let toolboxes =
            Toolboxes::open(self.home.images(), KernelMounter::new(self.home.mounts()))?;
        let toolbox = toolboxes.verify_incoming(self.release, &self.home.incoming())?;
        let resources = Mounted(MountedToolboxes::from(toolboxes));
        let config = BubblewrapConfig::for_host(
            toolbox,
            (paths.cgroup_root.clone(), paths.state_dir.clone()),
            self.entry,
            self.tuning,
            self.env,
        );
        Ok(Started {
            engine: BubblewrapEngine::new(config, host)?,
            resources: Box::new(resources),
        })
    }
}

/// The verified toolboxes every sandbox runs on, mounted until the host's
/// engine and sandboxes are gone.
#[derive(Debug)]
struct Mounted(MountedToolboxes<KernelMounter>);

impl Resources for Mounted {
    fn close(self: Box<Self>) -> Result<()> {
        Ok(self.0.close()?)
    }
}

/// Refuses a host that lacks a mechanism every sandbox needs, naming it.
///
/// # Errors
/// The first mechanism the probe found missing.
pub(super) fn admissible(host: &HostProbe) -> Result<()> {
    let Some(missing) = crate::BubblewrapRequirements::missing(host) else {
        return Ok(());
    };
    let error = refused(missing);
    let Logged { error_code, reason } = error.logged();
    let event = EVENT_HOST_REFUSED;
    {
        let message = format_args!("this host cannot build a sandbox");
        tracing::error!(message, missing, error_code, reason, event);
    };
    Err(error)
}
