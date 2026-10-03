//! One cgroup v2 per lease: its limits, and the kill that ends its whole tree.
//!
//! Plain writes to the control files the kernel publishes. `cgroups-rs` does
//! v2 and `cgroup.kill`, but carries a D-Bus client for its systemd manager;
//! the five files a lease needs do not justify that.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use backon::{BlockingRetryable as _, ConstantBuilder};

use crate::engine::Limits;
use crate::error::{Result, cgroup, cgroup_left};

/// The file that enables controllers for a cgroup's children.
pub const SUBTREE_CONTROL: &str = "cgroup.subtree_control";
/// Writing a process identifier moves that process in.
pub(crate) const CGROUP_PROCS: &str = "cgroup.procs";
/// Memory a cgroup may hold before the kernel reclaims or kills.
const MEMORY_MAX: &str = "memory.max";
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

/// One lease's cgroup. Removing it is the only cleanup, and it consumes it.
#[derive(Debug)]
pub struct LeaseCgroup {
    dir: PathBuf,
}

impl LeaseCgroup {
    /// Makes the cgroup `name` under the delegated `root` and writes `limits`.
    ///
    /// # Errors
    /// The directory cannot be made, or a control file refuses its value; the
    /// directory is removed again before the error returns.
    pub fn create(root: &Path, name: &str, limits: &Limits) -> Result<Self> {
        let made = Self {
            dir: root.join(name),
        };
        fs::create_dir(&made.dir)?;
        made.limit(limits).map_err(|error| made.undo(error))?;
        Ok(made)
    }

    /// Adopts a cgroup a previous run of this host left behind, to remove it.
    #[cfg(target_os = "linux")]
    pub(crate) fn leftover(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Removes a cgroup whose limits were refused, then hands the refusal back.
    /// Nothing has joined it yet, so the directory is all there is.
    fn undo(&self, refusal: crate::Error) -> crate::Error {
        if let Err(leftover) = fs::remove_dir(&self.dir) {
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

    /// The file a process writes its identifier to in order to join.
    #[must_use]
    pub fn procs(&self) -> PathBuf {
        self.dir.join(CGROUP_PROCS)
    }

    /// Kills every process in the cgroup, descendants included.
    ///
    /// # Errors
    /// The kernel refuses the write, as one older than 5.14 does.
    pub fn kill(&self) -> Result<()> {
        self.write(CGROUP_KILL, KILL)
    }

    /// Kills what remains, then removes the cgroup once its last process has
    /// gone. Blocks while the kernel reaps: call it off an async runtime.
    ///
    /// # Errors
    /// The kill is refused, or the cgroup is still busy after five seconds, or
    /// the kernel refuses the removal for another reason.
    pub fn remove(self) -> Result<()> {
        self.kill()?;
        // `EBUSY` is the kernel saying a process has not finished dying; every
        // other refusal is final.
        let busy = |error: &std::io::Error| error.kind() == std::io::ErrorKind::ResourceBusy;
        (|| fs::remove_dir(&self.dir))
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

    fn write(&self, file: &'static str, value: &str) -> Result<()> {
        fs::write(self.dir.join(file), value).map_err(cgroup(file))
    }
}

#[cfg(test)]
mod tests;
