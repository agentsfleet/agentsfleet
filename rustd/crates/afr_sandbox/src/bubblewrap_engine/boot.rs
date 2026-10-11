//! Bubblewrap's startup requirements, toolbox admission and resource lifetime.

use std::path::PathBuf;

use afd_core::env::EnvSource;
use afd_core::error_code::{Coded as _, Logged};

use super::{BubblewrapConfig, BubblewrapEngine, EVENT_HOST_REFUSED};
use crate::error::{Result, refused};
use crate::probe::HostProbe;
use crate::toolbox::{KernelMounter, MountedToolboxes, Release, ToolboxHome, Toolboxes};
use crate::{Backend, HostPaths, Resources, Started};

/// The release and entry point used by the shipping sandbox backend.
#[derive(Debug)]
pub struct BubblewrapBackend<'a, E: EnvSource> {
    /// Binary started inside each sandbox.
    pub entry: PathBuf,
    /// Identity that admits the staged toolbox.
    pub release: &'a Release,
    /// Images and mount locations supplied by the storage home.
    pub home: &'a ToolboxHome,
    /// Host settings the backend passes into its sandbox.
    pub env: &'a E,
}

impl<E: EnvSource> Backend for BubblewrapBackend<'_, E> {
    type Engine = BubblewrapEngine;

    fn validate(&self, host: &HostProbe) -> Result<()> {
        admissible(host)
    }

    fn start(self, paths: &HostPaths, host: &HostProbe) -> Result<Started<Self::Engine>> {
        let toolboxes =
            Toolboxes::open(self.home.images(), KernelMounter::new(self.home.mounts()))?;
        let toolbox = toolboxes.admit_incoming(self.release, &self.home.incoming())?;
        let resources = MountedToolboxes::from(toolboxes);
        let config = BubblewrapConfig::for_host(
            toolbox,
            paths.cgroup_root.clone(),
            paths.state_dir.clone(),
            self.entry,
            self.env,
        );
        Ok(Started {
            engine: BubblewrapEngine::new(config, host)?,
            resources: Box::new(resources),
        })
    }
}

impl Resources for MountedToolboxes<KernelMounter> {
    fn close(self: Box<Self>) -> Result<()> {
        (*self).close()
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
    tracing::error!(
        missing,
        error_code,
        reason,
        event,
        "this host cannot build a sandbox"
    );
    Err(error)
}
