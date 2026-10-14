//! The interface every engine meets.

use std::fmt;
use std::path::{Path, PathBuf};

use afr_executor::Executor;

use crate::error::Result;
use crate::network::{Allowlist, Network};

/// Memory a lease's sandbox may hold before its tree is killed.
pub const DEFAULT_MEMORY_BYTES: u64 = 2 * 1024 * 1024 * 1024;
/// Processor share, in thousandths of one core.
pub const DEFAULT_CPU_MILLIS: u32 = 2_000;
/// Processes and threads the sandbox's tree may hold at once.
pub const DEFAULT_PIDS: u32 = 512;
/// Size of the lease's workspace disk; a write past it answers `ENOSPC`.
pub const DEFAULT_DISK_BYTES: u64 = 4 * 1024 * 1024 * 1024;
/// The share of a sandbox's memory its `/dev/shm` may fill: one part in this
/// many. Shared memory is a `tmpfs`, so its pages stay charged to the tenant
/// after the process that wrote them dies; uncapped, one full `/dev/shm`
/// leaves every later command in the lease, and in a held sandbox the next
/// lease too, killed for memory at its first allocation.
const SHARED_MEMORY_SHARE: u64 = 4;

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

impl Limits {
    /// What the sandbox's `/dev/shm` may hold, in bytes: a full one answers
    /// `ENOSPC` while the tenant keeps the rest of its memory.
    #[must_use]
    pub const fn shared_memory_bytes(&self) -> u64 {
        self.memory_bytes / SHARED_MEMORY_SHARE
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
    /// What its network reaches.
    pub network: Network<'a>,
}

impl<'a> SandboxRequest<'a> {
    /// A sandbox for `lease_id` enforcing `limits`, reaching nothing beyond
    /// loopback until [`SandboxRequest::with_network`] says otherwise: an
    /// engine never opens a network no one asked for.
    #[must_use]
    pub const fn new(lease_id: &'a str, limits: Limits) -> Self {
        Self {
            lease_id,
            limits,
            network: Network::Isolated,
        }
    }

    /// The same request, its network reaching what `network` names.
    #[must_use]
    pub const fn with_network(self, network: Network<'a>) -> Self {
        Self { network, ..self }
    }

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

    /// Stops every process inside where it stands, keeping its memory and its
    /// workspace, until [`Sandbox::thaw`]; returns once nothing in it runs.
    /// Between two leases of one fleet the runner holds its sandbox frozen.
    ///
    /// # Errors
    /// The engine cannot stop the sandbox's processes, or cannot confirm they
    /// stopped. A sandbox that will not freeze is destroyed, never held.
    async fn freeze(&self) -> Result<()>;

    /// Lets every process a [`Sandbox::freeze`] stopped run on; returns once
    /// none is still stopped.
    ///
    /// # Errors
    /// As [`Sandbox::freeze`]. A sandbox that will not thaw is destroyed, and
    /// its lease gets a fresh one.
    async fn thaw(&self) -> Result<()>;

    /// Holds the sandbox to `allowlist` from now on, in place of the
    /// allowlist it was built to, and renders its names to match: what a held
    /// sandbox takes when its next lease's hosts resolved anew. The addresses
    /// change in one transaction, so no connection meets half of each set;
    /// call it while the sandbox is frozen, so nothing reads its names
    /// mid-write.
    ///
    /// # Errors
    /// The sandbox was built to no allowlist, the kernel refused the swap, or
    /// its names would not render. A sandbox that refuses is destroyed, never
    /// resumed.
    async fn reallow(&mut self, allowlist: &Allowlist) -> Result<()>;

    /// Ends every process inside, then removes what it held.
    ///
    /// Takes the sandbox by value: a destroyed sandbox cannot be used again,
    /// and the type system, not a flag, is what says so.
    async fn destroy(self: Box<Self>) -> Result<()>;
}

#[cfg(test)]
mod tests;
