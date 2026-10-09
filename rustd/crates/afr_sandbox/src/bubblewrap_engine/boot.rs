//! A runner host's boot, and the refusal of a host no sandbox can be built on.
//!
//! The host is probed first and refused before anything is mounted when it
//! lacks a mechanism every sandbox needs; then the toolbox release a deploy
//! staged is admitted, and the engine every lease runs in is built over it.
//! `agentsfleet-runner run` boots through here and so does the kernel lane,
//! so the lane proves the boot a host makes.

use std::path::PathBuf;

use afd_core::env::EnvSource;
use afd_core::error_code::{Coded as _, Logged};

use super::{BubblewrapConfig, BubblewrapEngine, EVENT_HOST_REFUSED};
use crate::error::{Result, refused};
use crate::probe::{HostProbe, ProbePaths, probe};
use crate::toolbox::{KernelMounter, MountedToolboxes, Release, ToolboxHome, Toolboxes};

/// What a booted host serves leases with. Its fields drop in order, so the
/// engine goes before the images its sandboxes run on.
#[derive(Debug)]
pub struct Booted {
    /// Builds each lease's sandbox.
    pub engine: BubblewrapEngine,
    /// What the host's kernel can enforce, as every heartbeat states it.
    pub probe: HostProbe,
    /// The toolbox images the engine's sandboxes run on.
    pub toolboxes: MountedToolboxes<KernelMounter>,
}

impl BubblewrapEngine {
    /// Boots a runner host: probes it under `cgroup_root` and `state_dir`,
    /// admits the release a deploy staged in `home` as `release` verifies it,
    /// and builds the engine over that toolbox, with `entry` starting every
    /// sandbox and the log level `env` names passed inside.
    ///
    /// # Errors
    /// The host lacks a mechanism every sandbox needs, refused before anything
    /// is mounted; the staged release is refused; or the engine is. A refusal
    /// past admission unmounts what was admitted.
    pub fn boot(
        cgroup_root: PathBuf,
        state_dir: PathBuf,
        entry: PathBuf,
        release: &Release,
        home: &ToolboxHome,
        env: &impl EnvSource,
    ) -> Result<Booted> {
        let host = probe(&ProbePaths {
            cgroup_root: cgroup_root.clone(),
            state_dir: Some(state_dir.clone()),
            ..ProbePaths::default()
        });
        admissible(&host)?;
        let toolboxes = Toolboxes::open(home.images(), KernelMounter::new(home.mounts()))?;
        // Admission mounts only what it admits, so a refusal leaves nothing to
        // unmount; past it, every early return drops the guard, which does.
        let toolbox = toolboxes.admit_incoming(release, &home.incoming())?;
        let toolboxes = MountedToolboxes::from(toolboxes);
        let config = BubblewrapConfig::for_host(toolbox, cgroup_root, state_dir, entry, env);
        Ok(Booted {
            engine: Self::new(config, &host)?,
            probe: host,
            toolboxes,
        })
    }
}

/// Refuses a host that lacks a mechanism every sandbox needs, naming it.
///
/// # Errors
/// The first mechanism the probe found missing.
pub(super) fn admissible(host: &HostProbe) -> Result<()> {
    let Some(missing) = host.missing() else {
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
