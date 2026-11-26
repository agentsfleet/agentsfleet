//! The `[firecracker]` section: where our Firecracker engine's programs and
//! guest artifacts live on this host, and where its jail users start.

use std::path::PathBuf;

use afr_engine_firecracker::{Binaries, FIRST_JAIL_ID, FirecrackerTuning};
use garde::Validate;
use serde::Deserialize;

/// The last first jail user and group the range still fits under: one id per
/// worker follows it, and `(uid_t) -1` means "no change" to every identity
/// call, so the range stops below it.
const FIRST_JAIL_ID_MAX: u32 = u32::MAX - 1 - af_core::limits::MAX_WORKERS;

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
    #[garde(range(max = FIRST_JAIL_ID_MAX))]
    pub first_jail_id: u32,
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
        }
    }
}

impl FirecrackerSection {
    /// How the engine starts and paces every microVM: these programs, paced
    /// as `sandbox` paces every engine's sandboxes.
    pub(super) fn tuning(&self, sandbox: afr_engine_bubblewrap::EngineTuning) -> FirecrackerTuning {
        FirecrackerTuning {
            binaries: Binaries {
                firecracker: self.firecracker.clone(),
                jailer: self.jailer.clone(),
                kernel: self.kernel.clone(),
                initrd: self.initrd.clone(),
            },
            first_jail_id: self.first_jail_id,
            ready_timeout: sandbox.ready_timeout,
            io_bytes_per_second: sandbox.io_bytes_per_second,
            sizes: sandbox.sizes,
        }
    }
}
