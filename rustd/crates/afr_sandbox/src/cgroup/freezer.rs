//! The cgroup v2 freezer: stopping every process of a lease where it stands,
//! and letting it run on.
//!
//! The lease's own cgroup is the one frozen, so its `sandbox` and `tenant`
//! leaves stop together: a frozen executor with a running tenant process, or
//! the reverse, never exists. The write is only a request; the kernel reports
//! the whole subtree settled through the `frozen` key of `cgroup.events`, and
//! nothing is called frozen before it says so
//! (<https://docs.kernel.org/admin-guide/cgroup-v2.html>, `cgroup.freeze`).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use backon::{BlockingRetryable as _, ConstantBuilder};

use crate::error::{Result, cgroup, cgroup_unsettled};

/// Writing `1` stops every process in the cgroup and its descendants; `0`
/// lets them run on.
const CGROUP_FREEZE: &str = "cgroup.freeze";
/// The cgroup's own state, its `frozen` key among it.
const CGROUP_EVENTS: &str = "cgroup.events";
/// The `cgroup.events` key that reports the freeze.
const FROZEN_KEY: &str = "frozen";
/// What `cgroup.freeze` is written, and `frozen` reads, when stopped.
const FROZEN: &str = "1";
/// What `cgroup.freeze` is written, and `frozen` reads, when running.
const THAWED: &str = "0";
/// The stopped state's name, for a refusal that says it was never reached.
const FROZEN_STATE: &str = "frozen";
/// The running state's name, likewise.
const THAWED_STATE: &str = "thawed";
/// How often a freeze or thaw is checked for having settled.
const SETTLE_POLL: Duration = Duration::from_millis(5);
/// How many checks before the kernel is taken not to settle: one second's
/// worth, since a freeze waits only for every task to reach a stop point.
const SETTLE_TRIES: usize = 200;

/// Freezes and thaws one lease's cgroup. A handle on its path alone, so it can
/// be moved off the async runtime while the sandbox keeps the cgroup.
#[derive(Debug, Clone)]
pub struct Freezer {
    dir: PathBuf,
}

impl Freezer {
    /// The freezer of the cgroup at `dir`.
    pub(crate) fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_owned(),
        }
    }

    /// Stops every process in the cgroup, and returns once the kernel reports
    /// the whole tree stopped. Blocks: call it off an async runtime.
    ///
    /// # Errors
    /// The kernel refuses the write, `cgroup.events` cannot be read, or the
    /// tree has not settled within a second.
    pub fn freeze(&self) -> Result<()> {
        self.settle(Settled::Frozen)
    }

    /// Lets every process in the cgroup run on, and returns once the kernel
    /// reports none still stopped. Blocks: call it off an async runtime.
    ///
    /// # Errors
    /// As [`Freezer::freeze`].
    pub fn thaw(&self) -> Result<()> {
        self.settle(Settled::Thawed)
    }

    /// Whether the kernel reports the cgroup frozen.
    ///
    /// # Errors
    /// `cgroup.events` cannot be read.
    pub fn is_frozen(&self) -> Result<bool> {
        let events =
            fs::read_to_string(self.dir.join(CGROUP_EVENTS)).map_err(cgroup(CGROUP_EVENTS))?;
        Ok(events
            .lines()
            .any(|line| line.split_once(' ') == Some((FROZEN_KEY, FROZEN))))
    }

    /// Writes `wanted`'s value to `cgroup.freeze`, then waits for `frozen` to
    /// read it back. A read that fails ends the wait at once; only a state not
    /// yet reached is waited out.
    fn settle(&self, wanted: Settled) -> Result<()> {
        fs::write(self.dir.join(CGROUP_FREEZE), wanted.value()).map_err(cgroup(CGROUP_FREEZE))?;
        let stopped = wanted == Settled::Frozen;
        let reached = || match self.is_frozen() {
            Ok(frozen) if frozen == stopped => Ok(()),
            Ok(_pending) => Err(None),
            Err(failed) => Err(Some(failed)),
        };
        reached
            .retry(
                ConstantBuilder::default()
                    .with_delay(SETTLE_POLL)
                    .with_max_times(SETTLE_TRIES),
            )
            .sleep(std::thread::sleep)
            .when(Option::is_none)
            .call()
            .map_err(|failed| failed.unwrap_or_else(|| cgroup_unsettled(&self.dir, wanted.name())))
    }
}

/// The state a freeze or a thaw asks the cgroup to settle in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Settled {
    /// Every process stopped.
    Frozen,
    /// Every process running.
    Thawed,
}

impl Settled {
    /// What `cgroup.freeze` is written, and `frozen` reads, in this state.
    const fn value(self) -> &'static str {
        match self {
            Self::Frozen => FROZEN,
            Self::Thawed => THAWED,
        }
    }

    /// The state's name, for the refusal that says it was never reached.
    const fn name(self) -> &'static str {
        match self {
            Self::Frozen => FROZEN_STATE,
            Self::Thawed => THAWED_STATE,
        }
    }
}

#[cfg(test)]
mod tests;
