//! What the Firecracker engine is, and what it grants one microVM.
//!
//! A guest's memory is its size and its processors are whole virtual CPUs,
//! so a grant rounds the ask up to what a monitor can be given — never down
//! — and states the rounded values. The process count is the guest's tenant
//! leaf's `pids.max`, which the guest entry writes from its command line.

use af_wire::runner::{Isolation, SandboxEngine};
use afr_engine::error::unsupported;
use afr_engine::{Capabilities, Granted, Limits, ParkMode, ResourceFacts, Result};

/// What the Firecracker engine is and guarantees every microVM it builds.
pub const CAPABILITIES: Capabilities = Capabilities {
    engine: SandboxEngine::Firecracker,
    isolation: Isolation::HardwareVm,
    parks: &[ParkMode::Stop],
    snapshots: false,
    resources: ResourceFacts::ENFORCED,
};

/// Thousandths of a core in one virtual CPU: [`Limits::cpu_millis`]'s unit.
pub(crate) const MILLIS_PER_VCPU: u32 = 1_000;
/// The most virtual CPUs one microVM takes: Firecracker's
/// `MAX_SUPPORTED_VCPUS`, which its machine configuration refuses past.
pub(crate) const MAX_VCPUS: u32 = 32;
/// The most processor share one microVM takes, in thousandths of a core:
/// `MAX_VCPUS` whole virtual CPUs, past which a grant is refused.
pub const MAX_CPU_MILLIS: u32 = MAX_VCPUS * MILLIS_PER_VCPU;
/// Bytes in a mebibyte: a monitor is given its guest's memory in whole MiB.
pub(crate) const MIB: u64 = 1024 * 1024;

/// `limits` as one microVM enforces them: processors rounded up to whole
/// virtual CPUs and memory up to whole mebibytes, each stated as rounded.
///
/// # Errors
/// More processors than one microVM takes, which is a cap this engine
/// cannot meet.
pub fn grant(limits: Limits) -> Result<Granted> {
    let vcpus = limits.cpu_millis.div_ceil(MILLIS_PER_VCPU).max(1);
    if vcpus > MAX_VCPUS {
        return Err(unsupported(SandboxEngine::Firecracker, "limit cpu"));
    }
    CAPABILITIES.grant(Limits {
        cpu_millis: vcpus * MILLIS_PER_VCPU,
        memory_bytes: limits.memory_bytes.div_ceil(MIB).max(1) * MIB,
        ..limits
    })
}

/// The virtual CPUs `granted` gives its guest.
#[must_use]
pub(crate) const fn vcpus(granted: &Granted) -> u32 {
    granted.limits().cpu_millis.div_euclid(MILLIS_PER_VCPU)
}

/// The guest memory `granted` gives, in mebibytes.
#[must_use]
pub(crate) const fn memory_mib(granted: &Granted) -> u64 {
    granted.limits().memory_bytes.div_euclid(MIB)
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::unwrap_used,
        reason = "a test asserts by panicking; the manifest's restriction set is for the runner"
    )]

    use afr_engine::{Cap, Limits};

    use super::{MAX_CPU_MILLIS, grant, memory_mib, vcpus};

    #[test]
    fn test_a_grant_rounds_up_to_whole_vcpus_and_mebibytes() {
        let asked = Limits {
            cpu_millis: 1_500,
            memory_bytes: 2 * 1024 * 1024 * 1024 + 1,
            ..Limits::DEFAULT
        };

        let granted = grant(asked).unwrap();

        assert_eq!(granted.limits().cpu_millis, 2_000);
        assert_eq!(vcpus(&granted), 2);
        assert_eq!(memory_mib(&granted), 2_049);
        assert_eq!(granted.limits().disk_bytes, asked.disk_bytes);
        assert_eq!(granted.limits().pids, asked.pids);
        assert_eq!(
            granted.facts().pids,
            Cap::Enforced,
            "the guest's tenant leaf holds the process count"
        );
        let a_sliver = grant(Limits {
            cpu_millis: 1,
            ..Limits::DEFAULT
        })
        .unwrap();
        assert_eq!(vcpus(&a_sliver), 1);
        let most = grant(Limits {
            cpu_millis: MAX_CPU_MILLIS,
            ..Limits::DEFAULT
        })
        .unwrap();
        assert_eq!(vcpus(&most), 32);
        let too_many = grant(Limits {
            cpu_millis: MAX_CPU_MILLIS + 1,
            ..Limits::DEFAULT
        })
        .unwrap_err();
        assert_eq!(too_many.unsupported_operation(), Some("limit cpu"));
    }
}
