//! Memory admission: the host admits at most its overcommitted bound of
//! microVMs at once, and refuses the next build or resume, naming memory,
//! before any monitor starts.
//!
//! ```text
//!   build or resume ─► admit ─► a reservation, held while the monitor runs
//!                         └──► past sandboxes_max: refused, no monitor
//!   park(Stop) or destroy ─► the reservation is dropped, and one is free
//! ```
//!
//! A pooled microVM is a built one, so it holds a reservation while it waits.

use std::sync::Arc;

use afr_engine::MemoryBound;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::error::memory_bound_reached;

/// Where the kernel states the memory it can give without swapping.
#[cfg(target_os = "linux")]
pub(crate) const MEMINFO_PATH: &str = "/proc/meminfo";
/// Why a kernel that does not state `MemAvailable` refuses the engine.
#[cfg(target_os = "linux")]
const NO_MEM_AVAILABLE: &str = "/proc/meminfo states no MemAvailable";

/// Counts the microVMs built against the host's bound.
#[derive(Debug)]
pub struct Admission {
    bound: MemoryBound,
    free: Arc<Semaphore>,
}

impl Admission {
    /// An admission holding the host to `bound`.
    #[must_use]
    pub fn new(bound: MemoryBound) -> Self {
        let permits = usize::try_from(bound.sandboxes_max)
            .unwrap_or(usize::MAX)
            .min(Semaphore::MAX_PERMITS);
        Self {
            bound,
            free: Arc::new(Semaphore::new(permits)),
        }
    }

    /// The bound it holds the host to.
    #[must_use]
    pub const fn bound(&self) -> MemoryBound {
        self.bound
    }

    /// One microVM's reservation, or the refusal naming memory when the
    /// bound's every reservation is held.
    ///
    /// # Errors
    /// `sandboxes_max` microVMs are built and none is parked or destroyed.
    pub fn admit(&self) -> afr_engine::Result<Reservation> {
        Arc::clone(&self.free)
            .try_acquire_owned()
            .map(|held| Reservation { _held: held })
            .map_err(|_every_one_held| memory_bound_reached(self.bound.sandboxes_max))
    }
}

/// One built microVM's share of the bound, freed when it is dropped: by a
/// `Stop` park, which keeps only the disk, or by a destroy.
#[derive(Debug)]
pub struct Reservation {
    _held: OwnedSemaphorePermit,
}

/// The host's `MemAvailable`, in bytes, read from `meminfo`.
///
/// # Errors
/// The file would not read or parse, or states no `MemAvailable`.
#[cfg(target_os = "linux")]
pub(crate) fn mem_available(meminfo: &std::path::Path) -> std::io::Result<u64> {
    use procfs_core::FromRead as _;

    procfs_core::Meminfo::from_file(meminfo)
        .map_err(std::io::Error::other)?
        .mem_available
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidData, NO_MEM_AVAILABLE))
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
    )]

    use std::num::NonZeroU64;

    use afr_engine::MemoryBound;

    use super::Admission;

    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * MIB;

    #[test]
    fn test_a_sandbox_past_the_memory_bound_is_refused() {
        let sandbox = NonZeroU64::new(2 * GIB + 16 * MIB).unwrap();
        let admission = Admission::new(MemoryBound::overcommitted(28 * GIB, 4, sandbox));
        let mut built: Vec<_> = (0..55).map(|_| admission.admit().unwrap()).collect();

        let refused = admission.admit().unwrap_err();
        let reason = refused.detail();
        assert!(reason.contains("memory admits 55"), "{reason}");
        // A `Stop` park keeps only the disk, and drops its reservation.
        drop(built.pop());
        built.push(admission.admit().unwrap());
        admission.admit().unwrap_err();
    }
}
