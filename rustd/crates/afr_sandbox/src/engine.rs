//! The interface every engine meets.

use std::fmt;
use std::path::{Path, PathBuf};

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

/// A sandbox's workspace as the host sees it: where it is mounted, and the
/// host user and group a file written into it must belong to so the
/// sandbox's processes own it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostWorkspace<'a> {
    /// The workspace's root on the host.
    pub root: &'a Path,
    /// The host user and group the sandbox's processes run as.
    pub owner: (u32, u32),
}

/// What a sandbox is built for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SandboxRequest<'a> {
    /// The lease it serves, which names its workspace and cgroup.
    pub lease_id: &'a str,
    /// What it enforces.
    pub limits: Limits,
}

impl<'a> SandboxRequest<'a> {
    /// The lease's identifier, checked as a name its directory and its cgroup
    /// can both carry.
    ///
    /// # Errors
    /// The identifier is not exactly one plain path segment, so it could name
    /// a directory outside its base or one shared with another lease.
    pub(crate) fn name(&self) -> Result<LeaseName<'a>> {
        LeaseName::parse(self.lease_id)
    }
}

/// A lease identifier that is one plain path segment: no separator, not empty,
/// not `.` or `..`. Checked once, so the directory and the cgroup named by it
/// are the same lease's.
///
/// A segment is checked as text rather than through `Path::components`, which
/// reads `x/` and `x/.` as the single segment `x`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LeaseName<'a>(&'a str);

impl<'a> LeaseName<'a> {
    /// The segments that name a directory without being one of their own.
    const RELATIVE: [&'static str; 2] = [".", ".."];

    /// `lease_id` as a name, or a refusal.
    fn parse(lease_id: &'a str) -> Result<Self> {
        let plain = !lease_id.is_empty()
            && !lease_id.contains(std::path::MAIN_SEPARATOR)
            && !lease_id.contains('\0')
            && !Self::RELATIVE.contains(&lease_id);
        if plain {
            Ok(Self(lease_id))
        } else {
            Err(crate::error::lease_id_unsafe(lease_id))
        }
    }

    /// The name as given.
    #[cfg_attr(
        not(target_os = "linux"),
        expect(
            dead_code,
            reason = "the bubblewrap engine, Linux only, names its cgroup by it"
        )
    )]
    pub(crate) const fn as_str(self) -> &'a str {
        self.0
    }

    /// The lease's own directory under `base`.
    pub(crate) fn dir_in(self, base: &Path) -> PathBuf {
        base.join(self.0)
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

    /// The workspace as the host sees it, so the supervisor can fill it
    /// before the turn runs; none for an engine whose workspace the host
    /// cannot reach, such as a microVM's disk.
    fn workspace(&self) -> Option<HostWorkspace<'_>> {
        None
    }

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
