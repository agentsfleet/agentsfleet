//! The boot sweep: what a previous run of this host left in the state
//! directory and in the host's network namespace, removed before this one
//! builds anything.
//!
//! A runner killed mid-lease leaves its sandboxes' cgroups, mounted workspace
//! disks and directories behind. Each lease directory's name is its cgroup's,
//! so one walk finds all three. What cannot be removed is logged and kept: a
//! disk that will not unmount keeps its image, and its directory, so a loop
//! device is never left on a file nobody can name.

use std::fs;
use std::path::Path;

use afd_core::error_code::{Coded as _, Logged};

use super::BubblewrapEngine;
use crate::cgroup::LeaseCgroup;
use crate::egress;
use crate::error::Result;
use crate::workspace_disk::WorkspaceDisk;

/// The event one swept lease is logged under.
const EVENT_SWEPT: &str = "sandbox_swept";
/// The event a lease the sweep could not remove is logged under.
const EVENT_SWEEP_FAILED: &str = "sandbox_sweep_failed";

impl BubblewrapEngine {
    /// Removes every lease a previous run left, logging each one, and, on a
    /// host that holds sandboxes to allowlists, every egress table and link.
    pub(super) fn sweep(&self, egress: bool) {
        if egress && let Err(error) = egress::sweep(&egress::Host) {
            let Logged { error_code, reason } = error.logged();
            let event = EVENT_SWEEP_FAILED;
            tracing::warn!(
                error_code,
                reason,
                event,
                "the egress sweep could not list what to remove"
            );
        }
        let Ok(entries) = fs::read_dir(&self.config.state_dir) else {
            return;
        };
        for entry in entries.flatten() {
            let dir = entry.path();
            if !dir.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let lease_id = name.to_string_lossy();
            match sweep_one(&dir, &self.config.cgroup_root.join(&name)) {
                Ok(()) => {
                    let event = EVENT_SWEPT;
                    tracing::info!(%lease_id, event);
                }
                Err(error) => {
                    let Logged { error_code, reason } = error.logged();
                    let event = EVENT_SWEEP_FAILED;
                    tracing::warn!(%lease_id, error_code, reason, event);
                }
            }
        }
    }
}

/// Kills what still runs, unmounts the disk, removes the directory: the order
/// the parts of a live sandbox are released in.
fn sweep_one(dir: &Path, cgroup: &Path) -> Result<()> {
    if cgroup.is_dir() {
        LeaseCgroup::leftover(cgroup.to_owned()).remove()?;
    }
    WorkspaceDisk::leftover(dir).release_leftover()?;
    fs::remove_dir_all(dir)?;
    Ok(())
}
