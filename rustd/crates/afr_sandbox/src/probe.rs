//! What this host's kernel can enforce, as the capability report states it.
//!
//! Every fact is read from a file the kernel publishes, at a path the caller
//! can replace, so a test states a host rather than needing one.

use std::fs;
use std::io::ErrorKind;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use crate::cgroup::SUBTREE_CONTROL;

/// Where the kernel lists its active security modules.
pub const LSM_PATH: &str = "/sys/kernel/security/lsm";
/// Where the kernel lists the file systems it can mount.
pub const FILESYSTEMS_PATH: &str = "/proc/filesystems";
/// Where the kernel lists the seccomp actions it supports.
pub const SECCOMP_ACTIONS_PATH: &str = "/proc/sys/kernel/seccomp/actions_avail";
/// The device a microVM engine opens.
pub const KVM_PATH: &str = "/dev/kvm";
/// Where cgroup v2 is mounted.
pub const CGROUP_ROOT: &str = "/sys/fs/cgroup";
/// Where Debian and Ubuntu install bubblewrap.
pub const BWRAP_PATH: &str = "/usr/bin/bwrap";

/// The mechanism names a refusal and the capability report share.
pub const MECHANISM_LANDLOCK: &str = "landlock";
/// See [`MECHANISM_LANDLOCK`].
pub const MECHANISM_SECCOMP: &str = "seccomp";
/// See [`MECHANISM_LANDLOCK`].
pub const MECHANISM_BUBBLEWRAP: &str = "bubblewrap";
/// See [`MECHANISM_LANDLOCK`]; the toolbox's file system is EROFS.
pub const MECHANISM_TOOLBOX_FILESYSTEM: &str = "erofs";
/// The cgroup controllers every lease's limits are written through.
pub const REQUIRED_CONTROLLERS: [&str; 3] = ["cpu", "memory", "pids"];

/// The seccomp action every refused system call answers with.
const SECCOMP_ERRNO_ACTION: &str = "errno";

/// Whether `/dev/kvm` exists, and whether this process may open it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kvm {
    /// No such device: a microVM engine cannot run here.
    Absent,
    /// The device exists, but this process may not open it for reading and
    /// writing.
    Denied,
    /// The device exists and opens: a microVM engine can run here.
    Usable,
}

/// One probe of the host, taken at boot and refreshed per heartbeat.
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is a separately reported mechanism, as on the wire's capability report"
)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostProbe {
    /// Landlock is enabled in the running kernel.
    pub landlock: bool,
    /// Seccomp filtering is available.
    pub seccomp: bool,
    /// Controllers enabled in the delegated cgroup's subtree.
    pub cgroup_controllers: Vec<String>,
    /// The bubblewrap launcher is installed and runs.
    pub bubblewrap: bool,
    /// What `/dev/kvm` allows.
    pub kvm: Kvm,
    /// The kernel can mount the toolbox's file system (EROFS); without it no
    /// sandbox can be built.
    pub toolbox_filesystem: bool,
}

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
    /// The delegated cgroup every lease's cgroup is made under.
    pub cgroup_root: PathBuf,
    /// The bubblewrap launcher.
    pub bwrap: PathBuf,
}

impl Default for ProbePaths {
    fn default() -> Self {
        Self {
            kvm: KVM_PATH.into(),
            filesystems: FILESYSTEMS_PATH.into(),
            lsm: LSM_PATH.into(),
            seccomp_actions: SECCOMP_ACTIONS_PATH.into(),
            cgroup_root: CGROUP_ROOT.into(),
            bwrap: BWRAP_PATH.into(),
        }
    }
}

/// Reads what this host can enforce. Never fails: a fact that cannot be read
/// is a mechanism the host does not have.
#[must_use]
pub fn probe(paths: &ProbePaths) -> HostProbe {
    let text = |path: &Path| fs::read_to_string(path).unwrap_or_default();
    HostProbe {
        landlock: text(&paths.lsm)
            .trim()
            .split(',')
            .any(|module| module == MECHANISM_LANDLOCK),
        seccomp: text(&paths.seccomp_actions)
            .split_whitespace()
            .any(|action| action == SECCOMP_ERRNO_ACTION),
        cgroup_controllers: text(&paths.cgroup_root.join(SUBTREE_CONTROL))
            .split_whitespace()
            .map(str::to_owned)
            .collect(),
        bubblewrap: fs::metadata(&paths.bwrap)
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0),
        kvm: kvm(&paths.kvm),
        toolbox_filesystem: text(&paths.filesystems)
            .lines()
            .any(|line| line.split_whitespace().last() == Some(MECHANISM_TOOLBOX_FILESYSTEM)),
    }
}

/// Opens the device for reading and writing, which is what a microVM needs.
fn kvm(path: &Path) -> Kvm {
    match fs::OpenOptions::new().read(true).write(true).open(path) {
        Ok(_device) => Kvm::Usable,
        Err(error) if error.kind() == ErrorKind::NotFound => Kvm::Absent,
        Err(_denied) => Kvm::Denied,
    }
}

impl HostProbe {
    /// The first mechanism every sandbox needs that this host lacks, or `None`
    /// when a sandbox can be built here.
    #[must_use]
    pub fn missing(&self) -> Option<&'static str> {
        [
            (self.landlock, MECHANISM_LANDLOCK),
            (self.seccomp, MECHANISM_SECCOMP),
            (self.bubblewrap, MECHANISM_BUBBLEWRAP),
            (self.toolbox_filesystem, MECHANISM_TOOLBOX_FILESYSTEM),
        ]
        .into_iter()
        .find_map(|(present, name)| (!present).then_some(name))
        .or_else(|| {
            REQUIRED_CONTROLLERS
                .into_iter()
                .find(|wanted| !self.cgroup_controllers.iter().any(|have| have == wanted))
        })
    }
}

#[cfg(test)]
mod tests;
