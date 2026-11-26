//! How many sandboxes an engine that reserves memory for each admits at once.
//!
//! A microVM reserves address space for its guest's memory, and the host
//! backs it only as the guest touches it, so a host may admit more guests
//! than its memory holds at their full size: `memory_overcommit` times as
//! many. The bound is fixed at boot from what the host had available then.

use std::num::{NonZeroU64, NonZeroU128};

/// The most sandboxes an engine admits at once under the host's memory, and
/// what that number was made from, as an operator reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryBound {
    /// The most sandboxes built at once; a parked one with only its disk
    /// kept holds none.
    pub sandboxes_max: u32,
    /// The host's `MemAvailable` at boot, in bytes.
    pub mem_available_bytes: u64,
    /// How many times its available memory the host admits.
    pub memory_overcommit: u32,
}

impl MemoryBound {
    /// `floor(memory_overcommit × mem_available_bytes ÷ sandbox_bytes)`
    /// sandboxes, each reserving `sandbox_bytes`: its guest memory and what
    /// its engine's own process takes beside it.
    #[must_use]
    pub fn overcommitted(
        mem_available_bytes: u64,
        memory_overcommit: u32,
        sandbox_bytes: NonZeroU64,
    ) -> Self {
        // In 128 bits, so no ratio times any 64-bit size overflows.
        let admitted = u128::from(memory_overcommit) * u128::from(mem_available_bytes)
            / NonZeroU128::from(sandbox_bytes);
        Self {
            sandboxes_max: u32::try_from(admitted).unwrap_or(u32::MAX),
            mem_available_bytes,
            memory_overcommit,
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

    use super::MemoryBound;

    const GIB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn test_the_bound_floors_and_never_wraps() {
        let one = NonZeroU64::new(1).unwrap();

        assert_eq!(
            MemoryBound::overcommitted(3 * GIB, 1, NonZeroU64::new(2 * GIB).unwrap()).sandboxes_max,
            1
        );
        assert_eq!(
            MemoryBound::overcommitted(u64::MAX, u32::MAX, one).sandboxes_max,
            u32::MAX
        );
        assert_eq!(MemoryBound::overcommitted(0, 4, one).sandboxes_max, 0);
    }
}
