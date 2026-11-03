//! How a lease's parts are released: [`Parts::teardown`] on the blocking
//! pool, or the drop of parts nobody tore down. Both run [`Parts::release`],
//! once.

use std::fs;

use afd_core::error_code::{Coded as _, Logged};

use super::{EVENT_TEARDOWN_COMPLETED, EVENT_TEARDOWN_FAILED, EVENT_TEARDOWN_STARTED, Parts};
use crate::egress::Host;
use crate::error::{Error, Result};

impl Parts {
    /// Ends every process, then releases the cgroup, the disk and the
    /// directory, off the async runtime: removing a cgroup waits for the
    /// kernel to reap.
    pub(in crate::bubblewrap_engine) async fn teardown(mut self) -> Result<()> {
        let lease_id = self.lease_id.as_str();
        let event = EVENT_TEARDOWN_STARTED;
        tracing::debug!(lease_id, event);
        if let Some(drain) = self.stderr.take() {
            drain.abort();
        }
        // The lease's name comes back with the outcome, so the release is
        // logged here, where the caller's subscriber is.
        let (released, lease_id) = tokio::task::spawn_blocking(move || {
            (self.release(), std::mem::take(&mut self.lease_id))
        })
        .await?;
        log_release(&lease_id, released.as_ref().err());
        released
    }

    /// Moves everything still held into parts of its own, leaving these empty.
    fn take(&mut self) -> Self {
        Self {
            lease_id: std::mem::take(&mut self.lease_id),
            dir: self.dir.take(),
            disk: self.disk.take(),
            cgroup: self.cgroup.take(),
            egress: self.egress.take(),
            child: self.child.take(),
            stderr: self.stderr.take(),
        }
    }

    /// Releases everything still held, in the one order that works: processes
    /// die before their cgroup goes and before their scope's rules do, so no
    /// packet leaves after its rules, and the disk is unmounted before its
    /// directory is removed. A disk that will not unmount keeps its directory,
    /// image and all, for the boot sweep: an attached loop device is never
    /// left on a file nobody can name.
    fn release(&mut self) -> Result<()> {
        let mut first = None;
        let mut keep = |step: Result<()>| {
            if let Err(error) = step {
                first.get_or_insert(error);
            }
        };
        if let Some(cgroup) = &self.cgroup {
            keep(cgroup.kill());
        }
        if let Some(mut child) = self.child.take() {
            // Usually already gone with its cgroup; a sandbox that died on its
            // own is not a teardown failure.
            if matches!(child.try_wait(), Ok(None)) {
                keep(child.start_kill().map_err(Error::from));
            }
        }
        if let Some(scope) = self.egress.take() {
            keep(scope.remove(&Host));
        }
        if let Some(cgroup) = self.cgroup.take() {
            keep(cgroup.remove());
        }
        let disk_released = self.disk.take().is_none_or(|disk| {
            let released = disk.release();
            let ok = released.is_ok();
            keep(released);
            ok
        });
        if let Some(dir) = self.dir.take().filter(|_| disk_released) {
            keep(fs::remove_dir_all(dir).map_err(Error::from));
        }
        first.map_or(Ok(()), Err)
    }
}

impl Drop for Parts {
    fn drop(&mut self) {
        // After a teardown nothing is left; after a cancelled start or a
        // sandbox nobody destroyed, this is the only cleanup there will be.
        let held = self.dir.is_some()
            || self.disk.is_some()
            || self.cgroup.is_some()
            || self.egress.is_some()
            || self.child.is_some();
        if !held {
            return;
        }
        let release = Release(self.take());
        // A cgroup can take seconds to empty. On a running runtime of either
        // flavor the release goes to its blocking pool, logging where this
        // thread logs, so a drop never holds a worker; with no runtime there
        // is no worker to hold. A stopping runtime's pool refuses the task by
        // dropping it, and the release runs wherever that drop lands.
        match tokio::runtime::Handle::try_current() {
            Ok(runtime) => {
                let logs = tracing::dispatcher::get_default(Clone::clone);
                drop(runtime.spawn_blocking(move || {
                    tracing::dispatcher::with_default(&logs, || drop(release));
                }));
            }
            Err(_no_runtime) => drop(release),
        }
    }
}

/// Parts on their way to [`Parts::release`], which runs when this drops: on
/// the blocking pool, or where a stopping pool drops the task it refused.
/// Released parts hold nothing, so their own drop ends there. Dropping held
/// parts instead would hand them to the same refusing pool, whose refusal
/// drops them again, until the stack runs out.
struct Release(Parts);

impl Drop for Release {
    fn drop(&mut self) {
        let released = self.0.release();
        log_release(&self.0.lease_id, released.as_ref().err());
    }
}

/// Logs how a release ended.
fn log_release(lease_id: &str, failed: Option<&Error>) {
    match failed {
        None => {
            let event = EVENT_TEARDOWN_COMPLETED;
            tracing::debug!(lease_id, event);
        }
        Some(error) => {
            let Logged { error_code, reason } = error.logged();
            let event = EVENT_TEARDOWN_FAILED;
            tracing::warn!(
                lease_id,
                error_code,
                reason,
                event,
                "a sandbox left something behind for the boot sweep"
            );
        }
    }
}
