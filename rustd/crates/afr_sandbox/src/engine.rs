//! The interface every engine meets.

use std::fmt;
use std::path::{Component, Path, PathBuf};

use afr_executor::Executor;

use crate::error::Result;

/// Memory a lease's sandbox may hold before its tree is killed.
pub const DEFAULT_MEMORY_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Processor share, in thousandths of one core.
pub const DEFAULT_CPU_MILLIS: u32 = 2_000;
/// Processes and threads the sandbox's tree may hold at once.
pub const DEFAULT_PIDS: u32 = 512;
/// Size of the lease's workspace disk; a write past it answers `ENOSPC`.
pub const DEFAULT_DISK_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// What one lease's sandbox enforces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Memory, in bytes.
    pub memory_bytes: u64,
    /// Processor share, in thousandths of one core.
    pub cpu_millis: u32,
    /// Processes and threads.
    pub pids: u32,
    /// The workspace disk's size, in bytes.
    pub disk_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            memory_bytes: DEFAULT_MEMORY_BYTES,
            cpu_millis: DEFAULT_CPU_MILLIS,
            pids: DEFAULT_PIDS,
            disk_bytes: DEFAULT_DISK_BYTES,
        }
    }
}

/// What a sandbox is built for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SandboxRequest<'a> {
    /// The lease it serves, which names its workspace and cgroup.
    pub lease_id: &'a str,
    /// What it enforces.
    pub limits: Limits,
}

impl SandboxRequest<'_> {
    /// The lease's own directory under `base`.
    ///
    /// # Errors
    /// The lease identifier is not exactly one plain path component, so it
    /// could name a directory outside `base` or one shared with another lease.
    pub fn lease_dir(&self, base: &Path) -> Result<PathBuf> {
        let mut parts = Path::new(self.lease_id).components();
        match (parts.next(), parts.next()) {
            (Some(Component::Normal(name)), None) => Ok(base.join(name)),
            _escapes => Err(crate::error::lease_id_unsafe(self.lease_id)),
        }
    }
}

/// Builds one sandbox per lease.
#[async_trait::async_trait]
pub trait Engine: Send + Sync + fmt::Debug {
    /// Builds a sandbox with its executor ready, or refuses.
    ///
    /// A sandbox that cannot be built refuses the lease; nothing ever runs
    /// unsandboxed in its place.
    async fn prepare(&self, request: SandboxRequest<'_>) -> Result<Box<dyn Sandbox>>;
}

/// One lease's sandbox, owned by the task running that lease.
#[async_trait::async_trait]
pub trait Sandbox: Send + Sync + fmt::Debug {
    /// The executor running inside it.
    fn executor(&self) -> &dyn Executor;

    /// Whether the sandbox is still up. A warm slot whose sandbox died while
    /// it waited is discarded on claim rather than handed to a lease; an
    /// engine with no process to watch keeps the default.
    fn is_running(&mut self) -> bool {
        true
    }

    /// Ends every process inside, then removes what it held.
    ///
    /// Takes the sandbox by value: a destroyed sandbox cannot be used again,
    /// and the type system, not a flag, is what says so.
    async fn destroy(self: Box<Self>) -> Result<()>;
}

#[cfg(test)]
mod tests;
