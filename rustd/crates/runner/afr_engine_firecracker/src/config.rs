//! What the Firecracker engine builds microVMs from: the binaries and guest
//! artifacts the runner's file names, and how it paces every boot.

use std::path::PathBuf;
use std::time::Duration;

use afr_engine::Limits;
use afr_executor::Sizes;

/// Where a runner host installs the monitor, by default.
pub const FIRECRACKER_PATH: &str = "/usr/local/bin/firecracker";
/// Where a runner host installs the jailer, by default.
pub const JAILER_PATH: &str = "/usr/local/bin/jailer";
/// Where a runner host keeps the guest kernel, by default.
pub const KERNEL_PATH: &str = "/usr/local/lib/agentsfleet/vmlinux";
/// Where a runner host keeps the guest's initial file system, whose `/init`
/// is the runner binary, by default.
pub const INITRD_PATH: &str = "/usr/local/lib/agentsfleet/guest.cpio";
/// The first jail user and group, by default.
///
/// It is past Debian's highest subordinate identifier (`SUB_UID_MAX` and
/// `SUB_GID_MAX`, 600100000, in `/etc/login.defs`), so no account or
/// container range on the host owns one.
pub const FIRST_JAIL_ID: u32 = 600_100_001;
/// How many times its available memory a host admits in guest memory, by
/// default: 4 is Indy's number (Indy, Nov 26, 2026: "Ratio 4"). A guest's
/// memory is backed only as it is touched.
pub const MEMORY_OVERCOMMIT: u32 = 4;
/// How many times its state directory's free space a host admits in
/// workspace disks, by default: 8 is Indy's number (Indy, Nov 27, 2026:
/// "8"). A workspace image is sparse, backed only as it is written.
pub const DISK_OVERCOMMIT: u32 = 8;

/// The programs and guest artifacts every boot uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binaries {
    /// The virtual machine monitor.
    pub firecracker: PathBuf,
    /// The jailer that starts each monitor in its own jail.
    pub jailer: PathBuf,
    /// The guest kernel.
    pub kernel: PathBuf,
    /// The guest's initial file system, the runner binary as its `/init`.
    pub initrd: PathBuf,
}

impl Default for Binaries {
    fn default() -> Self {
        Self {
            firecracker: FIRECRACKER_PATH.into(),
            jailer: JAILER_PATH.into(),
            kernel: KERNEL_PATH.into(),
            initrd: INITRD_PATH.into(),
        }
    }
}

/// How the engine starts and paces every microVM: what the runner's file
/// chooses for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirecrackerTuning {
    /// What every boot runs.
    pub binaries: Binaries,
    /// The first of the jail users and groups this host hands out, one per
    /// running microVM.
    pub first_jail_id: u32,
    /// How long a boot may take to answer before its run is refused.
    pub ready_timeout: Duration,
    /// Workspace-drive bytes a guest may read and write, together, per second.
    pub io_bytes_per_second: u64,
    /// What every guest's executor serves with and its client keeps the
    /// link to.
    pub sizes: Sizes,
    /// What every run's microVM is granted, from which one microVM's memory
    /// reservation is made.
    pub limits: Limits,
    /// How many times its available memory the host admits in guest memory.
    pub memory_overcommit: u32,
    /// How many times the free space under its state directory the host
    /// admits in workspace disks.
    pub disk_overcommit: u32,
}
