//! Shared host preparation; each backend owns admission and its resources.

use std::fmt;
use std::path::PathBuf;

use crate::cgroup::enable_controllers;
use crate::{Engine, HostProbe, ProbePaths, Result, probe};

/// Locations prepared before any backend admits its resources.
#[derive(Debug, Clone)]
pub struct HostPaths {
    /// Service root delegated by the host manager.
    pub cgroup_root: PathBuf,
    /// Storage for this host's lease sandboxes.
    pub state_dir: PathBuf,
}

/// Resources that must outlive every engine and sandbox using this host.
/// Implementations also release them on drop, including failed startup.
pub trait Resources: fmt::Debug + Send + Sync {
    /// Releases the resources after all engine and sandbox references are gone.
    ///
    /// # Errors
    /// A host resource could not be released.
    fn close(self: Box<Self>) -> Result<()>;
}

/// Startup policy and construction for one sandbox implementation.
pub trait Backend {
    /// The execution engine this backend constructs.
    type Engine: Engine;

    /// Checks this backend's requirements against measured host facts.
    ///
    /// # Errors
    /// A required mechanism is unavailable.
    fn validate(&self, host: &HostProbe) -> Result<()>;

    /// Acquires resources and constructs the engine on a prepared host.
    ///
    /// # Errors
    /// Resource admission or engine construction failed; acquired resources
    /// must be released before returning the error.
    fn start(self, paths: &HostPaths, host: &HostProbe) -> Result<Started<Self::Engine>>;
}

/// An engine and the backend's resources, dropped in that order.
#[derive(Debug)]
pub struct Started<E: Engine> {
    /// Builds each lease's sandbox.
    pub engine: E,
    /// Kept alive until every sandbox has stopped.
    pub resources: Box<dyn Resources>,
}

/// A prepared host and the backend it can enforce.
#[derive(Debug)]
pub struct Booted<E: Engine> {
    /// The engine and the resources it uses.
    pub started: Started<E>,
    /// Measured facts reported in heartbeats.
    pub probe: HostProbe,
}

/// Enables delegated controllers, measures the host, and admits one backend.
/// The caller supplies the service's delegated root, never a host ancestor.
///
/// # Errors
/// Delegation cannot be activated, or the selected backend refuses the host
/// or its resources. No fallback backend is selected on failure.
pub fn boot<B: Backend>(backend: B, paths: &HostPaths) -> Result<Booted<B::Engine>> {
    enable_controllers(&paths.cgroup_root)?;
    let host = probe(&ProbePaths {
        cgroup_root: paths.cgroup_root.clone(),
        state_dir: Some(paths.state_dir.clone()),
        ..ProbePaths::default()
    });
    backend.validate(&host)?;
    Ok(Booted {
        started: backend.start(paths, &host)?,
        probe: host,
    })
}

#[cfg(test)]
mod tests;
