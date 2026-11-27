//! Admission: the host admits at most its overcommitted bound of microVMs at
//! once, the lesser of what its memory and its disk admit, and refuses the
//! next build or resume, naming the resource that bound it, before any
//! monitor starts.
//!
//! ```text
//!   boot ─► MemAvailable, and the free space under the state directory
//!        ─► sandboxes_max: the lesser of what each admits, overcommitted
//!   build or resume ─► admit ─► a reservation, held while the monitor runs
//!                         └──► past sandboxes_max: refused, no monitor
//!   park(Stop) or destroy ─► the reservation is dropped, and one is free
//! ```
//!
//! A pooled microVM is a built one, so it holds a reservation while it waits.

use std::sync::Arc;

use afr_engine::AdmissionBound;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use crate::error::bound_reached;

/// Where the kernel states the memory it can give without swapping.
#[cfg(target_os = "linux")]
pub(crate) const MEMINFO_PATH: &str = "/proc/meminfo";
/// Why a kernel that does not state `MemAvailable` refuses the engine.
#[cfg(target_os = "linux")]
const NO_MEM_AVAILABLE: &str = "/proc/meminfo states no MemAvailable";

/// Counts the microVMs built against the host's bound.
#[derive(Debug)]
pub struct Admission {
    bound: AdmissionBound,
    free: Arc<Semaphore>,
}

impl Admission {
    /// An admission holding the host to `bound`.
    #[must_use]
    pub fn new(bound: AdmissionBound) -> Self {
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
    pub const fn bound(&self) -> AdmissionBound {
        self.bound
    }

    /// One microVM's reservation, or the refusal naming the resource that
    /// made the bound when its every reservation is held.
    ///
    /// # Errors
    /// `sandboxes_max` microVMs are built and none is parked or destroyed.
    pub fn admit(&self) -> afr_engine::Result<Reservation> {
        Arc::clone(&self.free)
            .try_acquire_owned()
            .map(|held| Reservation { _held: held })
            .map_err(|_every_one_held| {
                bound_reached(self.bound.bound_by(), self.bound.sandboxes_max)
            })
    }
}

/// One built microVM's share of the bound, freed when it is dropped: by a
/// `Stop` park, which keeps only the disk, or by a destroy.
#[derive(Debug)]
pub struct Reservation {
    _held: OwnedSemaphorePermit,
}

/// The bytes an unprivileged process may still write to the file system
/// holding `dir`: what a workspace image under it can be backed with.
///
/// # Errors
/// The file system would not state its free space.
#[cfg(target_os = "linux")]
pub(crate) fn disk_free(dir: &std::path::Path) -> std::io::Result<u64> {
    let stated = rustix::fs::statvfs(dir)?;
    Ok(stated.f_bavail.saturating_mul(stated.f_frsize))
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

    use afr_engine::{AdmissionBound, Overcommitted};

    use super::Admission;

    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * MIB;

    /// The bound 28 GiB of memory at 4 to 1 and `disk_free` bytes at 8 to 1
    /// make, for a 2 GiB guest with a 10 GiB workspace.
    fn bound(disk_free: u64) -> AdmissionBound {
        let memory = Overcommitted::of(28 * GIB, 4, NonZeroU64::new(2 * GIB + 16 * MIB).unwrap());
        let disk = Overcommitted::of(disk_free, 8, NonZeroU64::new(10 * GIB).unwrap());
        AdmissionBound::lesser(memory, disk)
    }

    #[test]
    fn test_a_sandbox_past_the_disk_bound_is_refused_naming_disk() {
        // 5 GiB free at 8 to 1 admits four 10 GiB workspaces; memory, 55.
        let admission = Admission::new(bound(5 * GIB));
        let _built: Vec<_> = (0..4).map(|_| admission.admit().unwrap()).collect();

        let refused = admission.admit().unwrap_err();

        let reason = refused.detail();
        assert!(reason.contains("disk admits 4"), "{reason}");
    }

    #[test]
    fn test_a_sandbox_past_the_memory_bound_is_refused() {
        let admission = Admission::new(bound(400 * GIB));
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
