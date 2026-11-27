//! How many sandboxes an engine that reserves memory and disk for each admits
//! at once.
//!
//! A microVM reserves address space for its guest's memory, and the host
//! backs it only as the guest touches it; its workspace image is sparse, and
//! the host backs it only as the guest writes it. So a host may admit more
//! guests than either resource holds at their full size: `memory_overcommit`
//! times as many by memory, `disk_overcommit` times as many by disk, and the
//! lesser of the two is the bound. It is fixed at boot from what the host had
//! of each then.

use std::num::{NonZeroU64, NonZeroU128};

use strum::IntoStaticStr;

/// A resource a host's sandboxes are counted against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum Resource {
    /// The host's available memory.
    Memory,
    /// The free space of the file system holding the engine's state.
    Disk,
}

/// What one resource admits, and what that number was made from, as an
/// operator reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Overcommitted {
    /// The most sandboxes the resource admits at once.
    pub sandboxes_max: u32,
    /// What the host had of the resource at boot, in bytes.
    pub available_bytes: u64,
    /// How many times that the host admits.
    pub overcommit: u32,
}

impl Overcommitted {
    /// `floor(overcommit × available_bytes ÷ sandbox_bytes)` sandboxes, each
    /// reserving `sandbox_bytes` of the resource.
    #[must_use]
    pub fn of(available_bytes: u64, overcommit: u32, sandbox_bytes: NonZeroU64) -> Self {
        // In 128 bits, so no ratio times any 64-bit size overflows.
        let admitted =
            u128::from(overcommit) * u128::from(available_bytes) / NonZeroU128::from(sandbox_bytes);
        Self {
            sandboxes_max: u32::try_from(admitted).unwrap_or(u32::MAX),
            available_bytes,
            overcommit,
        }
    }
}

/// The most sandboxes an engine admits at once: the lesser of what the
/// host's memory and its disk admit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AdmissionBound {
    /// The most sandboxes built at once; a parked one with only its disk
    /// kept holds none.
    pub sandboxes_max: u32,
    /// What the host's `MemAvailable` admits.
    pub memory: Overcommitted,
    /// What the free space under the engine's state admits.
    pub disk: Overcommitted,
}

impl AdmissionBound {
    /// The bound `memory` and `disk` make together: the lesser.
    #[must_use]
    pub fn lesser(memory: Overcommitted, disk: Overcommitted) -> Self {
        Self {
            sandboxes_max: memory.sandboxes_max.min(disk.sandboxes_max),
            memory,
            disk,
        }
    }

    /// The resource that admits fewer, which the bound is: memory when both
    /// admit the same.
    #[must_use]
    pub const fn bound_by(&self) -> Resource {
        if self.disk.sandboxes_max < self.memory.sandboxes_max {
            Resource::Disk
        } else {
            Resource::Memory
        }
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
    )]

    use std::num::NonZeroU64;

    use super::{AdmissionBound, Overcommitted, Resource};

    const GIB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn test_the_bound_floors_and_never_wraps() {
        let one = NonZeroU64::new(1).unwrap();

        assert_eq!(
            Overcommitted::of(3 * GIB, 1, NonZeroU64::new(2 * GIB).unwrap()).sandboxes_max,
            1
        );
        assert_eq!(
            Overcommitted::of(u64::MAX, u32::MAX, one).sandboxes_max,
            u32::MAX
        );
        assert_eq!(Overcommitted::of(0, 4, one).sandboxes_max, 0);
    }

    /// 28 GiB of memory at 4 to 1 admits 55 guests of 2 GiB and 16 MiB;
    /// 100 GiB of disk at 8 to 1 admits 80 of 10 GiB, and 40 GiB admits 32.
    #[test]
    fn test_the_bound_is_the_lesser_of_what_memory_and_disk_admit() {
        let memory = Overcommitted::of(28 * GIB, 4, NonZeroU64::new(2 * GIB + (16 << 20)).unwrap());
        let disk = |free| Overcommitted::of(free, 8, NonZeroU64::new(10 * GIB).unwrap());

        let roomy = AdmissionBound::lesser(memory, disk(100 * GIB));
        let full = AdmissionBound::lesser(memory, disk(40 * GIB));
        let even = AdmissionBound::lesser(memory, memory);

        assert_eq!((memory.sandboxes_max, roomy.disk.sandboxes_max), (55, 80));
        assert_eq!(
            (roomy.sandboxes_max, roomy.bound_by()),
            (55, Resource::Memory)
        );
        assert_eq!((full.sandboxes_max, full.bound_by()), (32, Resource::Disk));
        assert_eq!(even.bound_by(), Resource::Memory, "a tie names memory");
        assert_eq!(<&str>::from(full.bound_by()), "disk");
    }
}
