//! The `[firecracker]` section: where our Firecracker engine's programs and
//! guest artifacts live on this host, where its jail users start, how far it
//! overcommits memory and disk, the memory each monitor holds beside its
//! guest's, and how many microVMs it boots ahead of their runs.

use std::path::PathBuf;

use afr_engine_firecracker::{
    Binaries, DISK_OVERCOMMIT, FIRST_JAIL_ID, FirecrackerTuning, MEMORY_OVERCOMMIT,
    MONITOR_OVERHEAD_BYTES, WRITE_CACHE_BYTES,
};
use garde::Validate;
use serde::Deserialize;

use super::tuning::SandboxTuning;

/// The last first jail user and group the range still fits under: one id per
/// worker follows it, and `(uid_t) -1` means "no change" to every identity
/// call, so the range stops below it.
const FIRST_JAIL_ID_MAX: u32 = u32::MAX - 1 - af_core::limits::MAX_WORKERS;
/// The microVMs booted ahead of their runs, by default: 4 is Indy's number
/// (Indy, Nov 26, 2026: "Build now, pool of 4").
const WARM_POOL: u32 = 4;
/// One mebibyte.
const MIB: u64 = 1024 * 1024;

/// The programs every microVM boot runs, and its jail users.
#[derive(Debug, Clone, Deserialize, Validate)]
#[serde(default, deny_unknown_fields)]
pub(super) struct FirecrackerSection {
    /// The virtual machine monitor.
    #[garde(skip)]
    pub firecracker: PathBuf,
    /// The jailer that starts each monitor in a jail of its own.
    #[garde(skip)]
    pub jailer: PathBuf,
    /// The guest kernel.
    #[garde(skip)]
    pub kernel: PathBuf,
    /// The guest's initial file system, whose `/init` is this binary.
    #[garde(skip)]
    pub initrd: PathBuf,
    /// The first jail user and group; one per running microVM follows it.
    /// Never 0: that jail's monitor would run as root, and no jail holds root.
    #[garde(range(min = 1, max = FIRST_JAIL_ID_MAX))]
    pub first_jail_id: u32,
    /// How many times its available memory the host admits in guest memory.
    #[garde(range(min = 1))]
    pub memory_overcommit: u32,
    /// How many times the free space under its state directory the host
    /// admits in workspace disks.
    #[garde(range(min = 1))]
    pub disk_overcommit: u32,
    /// The microVMs booted ahead of their runs; each holds its share of the
    /// admission bound while it waits.
    #[garde(range(min = 1))]
    pub warm_pool: u32,
    /// What admission reserves for each monitor beyond its guest's memory,
    /// and where the monitor's cgroup throttles above it.
    #[garde(range(min = 1))]
    pub monitor_overhead_mib: u64,
    /// How far each monitor's cgroup cap sits above its throttle, for the
    /// host's cache of the guest's unsaved disk writes.
    #[garde(range(min = 1))]
    pub write_cache_mib: u64,
}

impl Default for FirecrackerSection {
    fn default() -> Self {
        let Binaries {
            firecracker,
            jailer,
            kernel,
            initrd,
        } = Binaries::default();
        Self {
            firecracker,
            jailer,
            kernel,
            initrd,
            first_jail_id: FIRST_JAIL_ID,
            memory_overcommit: MEMORY_OVERCOMMIT,
            disk_overcommit: DISK_OVERCOMMIT,
            warm_pool: WARM_POOL,
            monitor_overhead_mib: MONITOR_OVERHEAD_BYTES / MIB,
            write_cache_mib: WRITE_CACHE_BYTES / MIB,
        }
    }
}

impl FirecrackerSection {
    /// How the engine starts and paces every microVM: these programs, paced
    /// and granted as `sandbox` says, each executor serving `sizes`.
    pub(super) fn tuning(
        &self,
        sandbox: &SandboxTuning,
        sizes: afr_executor::Sizes,
    ) -> FirecrackerTuning {
        FirecrackerTuning {
            binaries: Binaries {
                firecracker: self.firecracker.clone(),
                jailer: self.jailer.clone(),
                kernel: self.kernel.clone(),
                initrd: self.initrd.clone(),
            },
            first_jail_id: self.first_jail_id,
            ready_timeout: sandbox.ready_timeout(),
            io_bytes_per_second: sandbox.io_bytes_per_second(),
            sizes,
            limits: sandbox.limits(),
            memory_overcommit: self.memory_overcommit,
            disk_overcommit: self.disk_overcommit,
            monitor_overhead_bytes: self.monitor_overhead_mib.saturating_mul(MIB),
            write_cache_bytes: self.write_cache_mib.saturating_mul(MIB),
        }
    }

    /// The microVMs booted ahead of their runs.
    pub(super) fn warm_pool(&self) -> usize {
        usize::try_from(self.warm_pool).unwrap_or(usize::MAX)
    }
}
