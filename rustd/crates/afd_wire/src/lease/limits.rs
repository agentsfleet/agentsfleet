//! The physical sandbox resources a fleet lease may request.

use garde::Validate;
use serde::{Deserialize, Serialize};

/// The smallest processor share a lease may ask for, in thousandths of a core.
pub const SANDBOX_CPU_MILLIS_MIN: u32 = 250;
/// The largest processor share a lease may ask for: 32 cores.
pub const SANDBOX_CPU_MILLIS_MAX: u32 = 32_000;
/// The least memory a lease may ask for. Above the sandbox's own reserve, so
/// the tenant's processes always get some.
pub const SANDBOX_MEMORY_BYTES_MIN: u64 = 256 * 1024 * 1024;
/// The most memory a lease may ask for: 64 GiB.
pub const SANDBOX_MEMORY_BYTES_MAX: u64 = 64 * 1024 * 1024 * 1024;
/// The smallest workspace disk a lease may ask for: 1 GiB.
pub const SANDBOX_DISK_BYTES_MIN: u64 = 1024 * 1024 * 1024;
/// The largest workspace disk a lease may ask for: 256 GiB.
pub const SANDBOX_DISK_BYTES_MAX: u64 = 256 * 1024 * 1024 * 1024;

/// The sandbox a lease asks for.
///
/// Bounded, because the runner builds exactly what it is told. A size past
/// these bounds is a daemon fault, so the runner refuses the lease. Whether
/// this host has room for it is a separate question.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct SandboxLimits {
    /// Processor share, in thousandths of one core.
    #[garde(range(min = SANDBOX_CPU_MILLIS_MIN, max = SANDBOX_CPU_MILLIS_MAX))]
    // utoipa takes only literals here; `validation_lease.rs` pins them to the
    // constants garde reads.
    #[cfg_attr(feature = "openapi", schema(minimum = 250, maximum = 32_000))]
    pub cpu_millis: u32,
    /// Memory, in bytes.
    #[garde(range(min = SANDBOX_MEMORY_BYTES_MIN, max = SANDBOX_MEMORY_BYTES_MAX))]
    #[cfg_attr(feature = "openapi", schema(minimum = 268_435_456_u64))]
    #[cfg_attr(feature = "openapi", schema(maximum = 68_719_476_736_u64))]
    pub memory_bytes: u64,
    /// The workspace disk's size, in bytes.
    #[garde(range(min = SANDBOX_DISK_BYTES_MIN, max = SANDBOX_DISK_BYTES_MAX))]
    #[cfg_attr(feature = "openapi", schema(minimum = 1_073_741_824_u64))]
    #[cfg_attr(feature = "openapi", schema(maximum = 274_877_906_944_u64))]
    pub disk_bytes: u64,
}
