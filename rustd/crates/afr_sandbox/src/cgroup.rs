//! One cgroup v2 per lease: its limits, and the kill that ends its whole tree.
//!
//! Plain writes to the control files the kernel publishes. `cgroups-rs` does
//! v2 and `cgroup.kill`, but carries a D-Bus client for its systemd manager;
//! the five files a lease needs do not justify that.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::engine::Limits;
use crate::error::{Result, cgroup};

/// The file that enables controllers for a cgroup's children.
pub(crate) const SUBTREE_CONTROL: &str = "cgroup.subtree_control";
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
/// Writing a process identifier moves that process in.
const CGROUP_PROCS: &str = "cgroup.procs";
/// Reports whether any process remains.
const CGROUP_EVENTS: &str = "cgroup.events";
/// The controllers this cgroup has.
const CGROUP_CONTROLLERS: &str = "cgroup.controllers";
/// The controller `io.max` belongs to.
const IO_CONTROLLER: &str = "io";
/// The line `cgroup.events` carries once the last process has gone.
const DRAINED: &str = "populated 0";
/// The event a cgroup left behind after a failed create is logged under.
const EVENT_CGROUP_LEFT: &str = "sandbox_cgroup_left";
/// The bandwidth period every quota is a share of.
const CPU_PERIOD_MICROS: u64 = 100_000;
/// Thousandths of a core in one core.
const MILLIS_PER_CORE: u64 = 1_000;
/// How often a killed cgroup is checked for its last process.
const DRAIN_POLL: Duration = Duration::from_millis(5);
/// How long a killed cgroup may take to empty before removal gives up.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(5);

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

    /// Removes a cgroup whose limits were refused, then hands the refusal back.
    /// Nothing has joined it yet, so the directory is all there is.
    fn undo(&self, refusal: crate::Error) -> crate::Error {
        if let Err(leftover) = fs::remove_dir(&self.dir) {
            let reason = leftover.to_string();
            let event = EVENT_CGROUP_LEFT;
            tracing::warn!(
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
            self.write(MEMORY_SWAP_MAX, "0")?;
        }
        Ok(())
    }

    /// Caps reads and writes to the block device `major:minor`, when this
    /// cgroup has the `io` controller; a host without it keeps the other limits.
    ///
    /// # Errors
    /// The controller is present and `io.max` refuses the value.
    pub fn limit_io(&self, device: (u32, u32), bytes_per_second: u64) -> Result<()> {
        let controllers = fs::read_to_string(self.dir.join(CGROUP_CONTROLLERS)).unwrap_or_default();
        if !controllers
            .split_whitespace()
            .any(|name| name == IO_CONTROLLER)
        {
            return Ok(());
        }
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
        self.write(CGROUP_KILL, "1")
    }

    /// Kills what remains, waits for it to go, then removes the cgroup.
    ///
    /// # Errors
    /// The kill is refused, the last process outlives the wait, or the kernel
    /// refuses the removal.
    pub async fn remove(self) -> Result<()> {
        self.kill()?;
        let events = self.dir.join(CGROUP_EVENTS);
        tokio::time::timeout(DRAIN_TIMEOUT, async {
            while !drained(&events) {
                tokio::time::sleep(DRAIN_POLL).await;
            }
        })
        .await
        .map_err(|elapsed| cgroup(CGROUP_EVENTS)(elapsed.into()))?;
        fs::remove_dir(&self.dir).map_err(cgroup(CGROUP_PROCS))
    }

    fn write(&self, file: &'static str, value: &str) -> Result<()> {
        fs::write(self.dir.join(file), value).map_err(cgroup(file))
    }
}

/// Whether the last process has left; an unreadable file means nothing is
/// left to wait for.
fn drained(events: &Path) -> bool {
    fs::read_to_string(events).map_or(true, |text| text.contains(DRAINED))
}

#[cfg(test)]
mod tests;
