//! How the bubblewrap engine starts and paces every sandbox it builds: what
//! the runner's file chooses for it.

use std::time::Duration;

use afr_executor::Sizes;

/// How an engine starts and paces every sandbox it builds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineTuning {
    /// How long a new sandbox may take to answer before its run is refused:
    /// the kernel lane measures executor-ready in tens of milliseconds, so
    /// the default only ends a sandbox that will never answer.
    pub ready_timeout: Duration,
    /// Disk bytes a run's sandbox may read, and write, per second.
    pub io_bytes_per_second: u64,
    /// What every sandbox's executor serves with and its client keeps the
    /// link to: the engine starts the one and connects the other with these.
    pub sizes: Sizes,
}

impl EngineTuning {
    /// The pacing every engine used before the runner's file could choose it.
    pub const DEFAULT: Self = Self {
        ready_timeout: Duration::from_secs(10),
        io_bytes_per_second: afr_cgroup::DEFAULT_IO_BYTES_PER_SECOND,
        sizes: Sizes::DEFAULT,
    };
}
