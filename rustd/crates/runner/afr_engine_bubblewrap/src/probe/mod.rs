//! What this host's kernel can enforce for bubblewrap, as the capability
//! report states it: the evidence the engine's boot admission judges.
//!
//! Each building block probes its own mechanism — Landlock and seccomp
//! ([`afr_guest`]), the delegated controllers ([`afr_cgroup`]), the toolbox's
//! file system and direct I/O ([`afr_image`]), an egress scope
//! ([`afr_firewall`]) — and this module assembles them, with the launcher and
//! the KVM device, into one [`HostProbe`]. Every fact is read from a path the
//! caller can replace, so a test states a host rather than needing one.

use std::fs;
use std::io::ErrorKind;
use std::path::PathBuf;

use af_wire::runner::Kvm;
use afr_engine::{HostPaths, HostProbe, Result};

/// The device a microVM engine opens.
pub const KVM_PATH: &str = "/dev/kvm";

/// Where [`probe`] reads each fact from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbePaths {
    /// The KVM device.
    pub kvm: PathBuf,
    /// The kernel's file-system list.
    pub filesystems: PathBuf,
    /// The kernel's security-module list.
    pub lsm: PathBuf,
    /// The kernel's seccomp action list.
    pub seccomp_actions: PathBuf,
    /// The delegated cgroup every run's cgroup is made under.
    pub cgroup_root: PathBuf,
    /// The bubblewrap launcher.
    pub bwrap: PathBuf,
    /// Where each run's workspace image is made; `None` when no engine is
    /// configured, and then the probe says nothing about direct I/O.
    pub state_dir: Option<PathBuf>,
}

impl Default for ProbePaths {
    fn default() -> Self {
        Self {
            kvm: KVM_PATH.into(),
            filesystems: afr_image::FILESYSTEMS_PATH.into(),
            lsm: afr_guest::LSM_PATH.into(),
            seccomp_actions: afr_guest::SECCOMP_ACTIONS_PATH.into(),
            cgroup_root: afr_cgroup::CGROUP_ROOT.into(),
            bwrap: afr_image::BWRAP_PATH.into(),
            state_dir: None,
        }
    }
}

/// What the kernel builds when asked, beyond what its files list: the
/// sandbox's Landlock ruleset, and an egress scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Enforceable {
    /// The kernel builds the ruleset the sandbox needs, not only lists
    /// Landlock among its modules.
    pub(crate) landlock: bool,
    /// The kernel builds and removes an egress scope.
    pub(crate) egress: bool,
}

impl Enforceable {
    /// Asks this host's kernel to build each.
    fn measure() -> Self {
        Self {
            landlock: afr_guest::landlock_enforceable(),
            egress: afr_firewall::enforceable(),
        }
    }
}

/// Reads what this host can enforce. Never fails: a fact that cannot be read
/// is a mechanism the host does not have.
#[must_use]
pub fn probe(paths: &ProbePaths) -> HostProbe {
    read(paths, Enforceable::measure())
}

/// Enables the delegated controllers under `paths`' cgroup root, then probes
/// the host there with its state directory: what bubblewrap's boot admission
/// judges.
///
/// # Errors
/// A required controller is not delegated, or activation is refused.
pub fn measure(paths: &HostPaths) -> Result<HostProbe> {
    afr_cgroup::enable_controllers(&paths.cgroup_root)?;
    Ok(probe(&ProbePaths {
        cgroup_root: paths.cgroup_root.clone(),
        state_dir: Some(paths.state_dir.clone()),
        ..ProbePaths::default()
    }))
}

/// [`probe`], told what the kernel builds rather than asking it, so a test
/// states a host it is not running on.
pub(crate) fn read(paths: &ProbePaths, enforceable: Enforceable) -> HostProbe {
    HostProbe {
        landlock: enforceable.landlock && afr_guest::landlock_listed(&paths.lsm),
        seccomp: afr_guest::seccomp_offered(&paths.seccomp_actions),
        cgroup_controllers: afr_cgroup::delegated_controllers(&paths.cgroup_root),
        // A file this process may execute: the permission bits alone say
        // nothing about who may.
        bubblewrap: fs::metadata(&paths.bwrap).is_ok_and(|meta| meta.is_file())
            && rustix::fs::access(&paths.bwrap, rustix::fs::Access::EXEC_OK).is_ok(),
        kvm: kvm(&paths.kvm),
        toolbox_filesystem: afr_image::toolbox_filesystem(&paths.filesystems),
        workspace_direct_io: paths.state_dir.as_deref().map(afr_image::direct_io),
        egress: enforceable.egress,
    }
}

/// Opens the device for reading and writing, which is what a microVM needs.
fn kvm(path: &std::path::Path) -> Kvm {
    match fs::OpenOptions::new().read(true).write(true).open(path) {
        Ok(_device) => Kvm::Usable,
        Err(error) if error.kind() == ErrorKind::NotFound => Kvm::Absent,
        Err(_denied) => Kvm::Denied,
    }
}

#[cfg(test)]
mod tests;
