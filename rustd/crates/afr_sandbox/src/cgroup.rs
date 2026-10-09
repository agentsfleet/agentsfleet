//! One cgroup v2 per lease: its limits, its two leaves, and the kill that ends
//! its whole tree.
//!
//! The lease's cgroup holds the limits and no process. Bubblewrap and the
//! executor run in its `sandbox` leaf; every process the executor starts runs
//! in its `tenant` leaf, whose memory limit sits [`SANDBOX_MEMORY_RESERVE_BYTES`]
//! below the lease's, so the kernel's out-of-memory killer chooses among
//! tenant processes and never the sandbox's own — systemd's `Delegate=` shape,
//! where a manager's processes and the subtree it hands out never share a leaf.
//!
//! Plain writes to the control files the kernel publishes. `cgroups-rs` does
//! v2 and `cgroup.kill`, but carries a D-Bus client for its systemd manager;
//! the few files a lease needs do not justify that.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use backon::{BlockingRetryable as _, ConstantBuilder};

use crate::engine::Limits;
use crate::error::{Result, cgroup, cgroup_left};
use crate::probe::REQUIRED_CONTROLLERS;

mod delegated;
mod freezer;

pub use self::delegated::{MECHANISM_DELEGATED_CGROUP, SELF_CGROUP_PATH, delegated_root};
pub use self::freezer::Freezer;

/// The file that enables controllers for a cgroup's children.
pub const SUBTREE_CONTROL: &str = "cgroup.subtree_control";
/// Writing a process identifier moves that process in.
pub(crate) const CGROUP_PROCS: &str = "cgroup.procs";
/// Memory a cgroup may hold before the kernel reclaims or kills.
const MEMORY_MAX: &str = "memory.max";
/// Memory past which the kernel slows a cgroup's allocations and reclaims,
/// rather than kills.
const MEMORY_HIGH: &str = "memory.high";
/// The tenant's throttle sits this share of its limit below it: room for
/// pages already handed to the disk to finish writing, so a tenant writing
/// past its disk is slowed until it reads `ENOSPC`, not killed first.
const TENANT_HIGH_SHARE: u64 = 8;
/// The most headroom that share may take. Write-back needs only so much room,
/// and with no swap every byte inside the band is throttled, so an eighth of a
/// large lease would slow allocation-heavy work across a wide slow band.
const TENANT_HIGH_HEADROOM_MAX_BYTES: u64 = 128 * 1024 * 1024;
/// Swap a cgroup may use; zero, so a runaway is killed rather than paged out.
const MEMORY_SWAP_MAX: &str = "memory.swap.max";
/// Processor bandwidth: a quota per period, both in microseconds.
const CPU_MAX: &str = "cpu.max";
/// Processes and threads the cgroup may hold.
const PIDS_MAX: &str = "pids.max";
/// Per-device byte rates.
const IO_MAX: &str = "io.max";
/// Writing `1` kills every process in the cgroup and its descendants.
const CGROUP_KILL: &str = "cgroup.kill";
/// What a cgroup's memory limit did, its `oom_kill` count among it.
pub(crate) const MEMORY_EVENTS: &str = "memory.events";
/// The leaf bubblewrap and the executor run in.
pub const SANDBOX_LEAF: &str = "sandbox";
/// The leaf every process the executor starts runs in.
pub const TENANT_LEAF: &str = "tenant";
/// Both leaves, in the order they are made.
const LEAVES: [&str; 2] = [SANDBOX_LEAF, TENANT_LEAF];
/// What `cgroup.kill` and a zeroed `memory.swap.max` are written.
const KILL: &str = "1";
/// No swap at all.
const NO_SWAP: &str = "0";
/// The event a cgroup left behind after a failed create is logged under.
const EVENT_CGROUP_LEFT: &str = "sandbox_cgroup_left";
/// The bandwidth period every quota is a share of.
const CPU_PERIOD_MICROS: u64 = 100_000;
/// Thousandths of a core in one core.
const MILLIS_PER_CORE: u64 = 1_000;
/// How often a killed cgroup's removal is retried while its last process dies.
const DRAIN_POLL: Duration = Duration::from_millis(5);
/// How many retries before removal gives up: five seconds' worth.
const DRAIN_TRIES: usize = 1_000;

/// Workspace-disk throughput, each way, in bytes per second.
pub const DEFAULT_IO_BYTES_PER_SECOND: u64 = 200 * 1024 * 1024;
/// Memory the sandbox's own processes keep when the tenant's are at their
/// limit: room for the executor's resident set and bubblewrap's.
pub const SANDBOX_MEMORY_RESERVE_BYTES: u64 = 64 * 1024 * 1024;

/// One lease's cgroup. Removing it is the only cleanup, and it consumes it.
#[derive(Debug)]
pub struct LeaseCgroup {
    dir: PathBuf,
}

impl LeaseCgroup {
    /// Makes the cgroup `name` under the delegated `root`, writes `limits`,
    /// and splits it into its two leaves.
    ///
    /// # Errors
    /// A directory cannot be made, or a control file refuses its value; what
    /// was made is removed again before the error returns.
    pub fn create(root: &Path, name: &str, limits: &Limits) -> Result<Self> {
        let made = Self {
            dir: root.join(name),
        };
        fs::create_dir(&made.dir)?;
        made.limit(limits)
            .and_then(|()| made.split(limits))
            .map_err(|error| made.undo(error))?;
        Ok(made)
    }

    /// Adopts a cgroup a previous run of this host left behind, to remove it.
    #[cfg(target_os = "linux")]
    pub(crate) fn leftover(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Removes a cgroup whose limits were refused, then hands the refusal back.
    /// Nothing has joined it yet, so its directories are all there is.
    fn undo(&self, refusal: crate::Error) -> crate::Error {
        if let Err(leftover) = self.remove_dirs() {
            let error_code = refusal.code().as_str();
            let reason = leftover.to_string();
            let event = EVENT_CGROUP_LEFT;
            tracing::warn!(
                error_code,
                reason,
                event,
                "a cgroup whose limits failed could not be removed"
            );
        }
        refusal
    }

    fn limit(&self, limits: &Limits) -> Result<()> {
        let quota = u64::from(limits.cpu_millis) * CPU_PERIOD_MICROS / MILLIS_PER_CORE;
        self.write(MEMORY_MAX, &limits.memory_bytes.to_string())?;
        self.write(CPU_MAX, &format!("{quota} {CPU_PERIOD_MICROS}"))?;
        self.write(PIDS_MAX, &limits.pids.to_string())?;
        // Present only where the kernel accounts swap; absent, there is no swap
        // for a runaway to hide in.
        if self.dir.join(MEMORY_SWAP_MAX).exists() {
            self.write(MEMORY_SWAP_MAX, NO_SWAP)?;
        }
        Ok(())
    }

    /// Hands the lease's controllers to its two leaves, caps the tenant
    /// leaf's memory below the lease's, and throttles it a bounded headroom
    /// below that cap. The lease's own swap limit already covers both leaves.
    fn split(&self, limits: &Limits) -> Result<()> {
        let enable = REQUIRED_CONTROLLERS.map(|controller| format!("+{controller}"));
        self.write(SUBTREE_CONTROL, &enable.join(" "))?;
        LEAVES
            .into_iter()
            .try_for_each(|leaf| fs::create_dir(self.dir.join(leaf)))?;
        let tenant = limits
            .memory_bytes
            .saturating_sub(SANDBOX_MEMORY_RESERVE_BYTES);
        let high = tenant - (tenant / TENANT_HIGH_SHARE).min(TENANT_HIGH_HEADROOM_MAX_BYTES);
        fs::write(self.tenant().join(MEMORY_MAX), tenant.to_string())
            .map_err(cgroup(MEMORY_MAX))?;
        fs::write(self.tenant().join(MEMORY_HIGH), high.to_string()).map_err(cgroup(MEMORY_HIGH))
    }

    /// Caps reads and writes to the block device `major:minor`.
    ///
    /// # Errors
    /// `io.max` refuses the value; the `io` controller is required, so a host
    /// without it never builds a sandbox to reach here.
    pub fn limit_io(&self, device: (u32, u32), bytes_per_second: u64) -> Result<()> {
        let (major, minor) = device;
        self.write(
            IO_MAX,
            &format!("{major}:{minor} rbps={bytes_per_second} wbps={bytes_per_second}"),
        )
    }

    /// The file a process writes its identifier to in order to join the
    /// sandbox leaf: bubblewrap's, and so the executor's.
    #[must_use]
    pub fn procs(&self) -> PathBuf {
        self.dir.join(SANDBOX_LEAF).join(CGROUP_PROCS)
    }

    /// The file a process writes its identifier to in order to join the
    /// tenant leaf.
    #[must_use]
    pub fn tenant_procs(&self) -> PathBuf {
        self.tenant().join(CGROUP_PROCS)
    }

    /// The tenant leaf's `memory.events`, whose `oom_kill` count rises with
    /// each tenant process the kernel kills for memory.
    #[must_use]
    pub fn tenant_events(&self) -> PathBuf {
        self.tenant().join(MEMORY_EVENTS)
    }

    fn tenant(&self) -> PathBuf {
        self.dir.join(TENANT_LEAF)
    }

    /// The handle that freezes and thaws this cgroup, both leaves at once.
    #[must_use]
    pub fn freezer(&self) -> Freezer {
        Freezer::new(&self.dir)
    }

    /// Kills every process in the cgroup, descendants included.
    ///
    /// # Errors
    /// The kernel refuses the write, as one older than 5.14 does.
    pub fn kill(&self) -> Result<()> {
        self.write(CGROUP_KILL, KILL)
    }

    /// Kills what remains, then removes the cgroup, leaves first, once its last
    /// process has gone. Blocks while the kernel reaps: call it off an async
    /// runtime.
    ///
    /// # Errors
    /// The kill is refused, or the cgroup is still busy after five seconds, or
    /// the kernel refuses the removal for another reason.
    pub fn remove(self) -> Result<()> {
        self.kill()?;
        // `EBUSY` is the kernel saying a process has not finished dying; every
        // other refusal is final.
        let busy = |error: &std::io::Error| error.kind() == std::io::ErrorKind::ResourceBusy;
        (|| self.remove_dirs())
            .retry(
                ConstantBuilder::default()
                    .with_delay(DRAIN_POLL)
                    .with_max_times(DRAIN_TRIES),
            )
            .sleep(std::thread::sleep)
            .when(busy)
            .call()
            .map_err(cgroup_left(&self.dir))
    }

    /// Removes whichever leaves exist, then the cgroup: a cgroup with
    /// children cannot go, and a crash between making the leaves and using
    /// them may have left either.
    fn remove_dirs(&self) -> std::io::Result<()> {
        LEAVES
            .into_iter()
            .map(|leaf| self.dir.join(leaf))
            .filter(|leaf| leaf.is_dir())
            .try_for_each(fs::remove_dir)?;
        fs::remove_dir(&self.dir)
    }

    fn write(&self, file: &'static str, value: &str) -> Result<()> {
        fs::write(self.dir.join(file), value).map_err(cgroup(file))
    }
}

#[cfg(test)]
mod tests;
