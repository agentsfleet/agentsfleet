//! The boot sweep: what a previous run of this host left in the state
//! directory and in the host's network namespace, removed before this one
//! builds anything.
//!
//! A runner killed mid-run leaves its sandboxes' cgroups, mounted workspace
//! disks and directories behind. Each sandbox directory's name is its cgroup's,
//! so one walk finds all three. What cannot be removed is logged and kept: a
//! disk that will not unmount keeps its image, and its directory, so a loop
//! device is never left on a file nobody can name.

use std::fs;
use std::path::Path;

use af_core::error_code::{Coded as _, Logged};
use af_core::id::Uuid7;
use afr_cgroup::RunCgroup;
use afr_engine::{Result, Swept};
use afr_image::WorkspaceDisk;

use super::BubblewrapEngine;

/// The event one swept run is logged under.
const EVENT_SWEPT: &str = "sandbox_swept";
/// The event a run the sweep could not remove is logged under.
const EVENT_SWEEP_FAILED: &str = "sandbox_sweep_failed";

impl BubblewrapEngine {
    /// Removes every sandbox a previous runner left, logging each one, and,
    /// on a host that holds sandboxes to allowlists, every egress table and
    /// link `egress` removes.
    pub(super) fn sweep_leftovers(
        &self,
        egress: Option<impl FnOnce() -> afr_firewall::Result<()>>,
    ) -> Swept {
        if let Some(egress) = egress
            && let Err(error) = egress()
        {
            let Logged { error_code, reason } = error.logged();
            let event = EVENT_SWEEP_FAILED;
            {
                let (message,) = (format_args!(
                    "the egress sweep could not list what to remove"
                ),);
                tracing::warn!(message, error_code, reason, event);
            };
        }
        let Ok(entries) = fs::read_dir(&self.config.state_dir) else {
            return Swept::default();
        };
        let mut swept = Swept::default();
        for entry in entries.flatten() {
            let dir = entry.path();
            if dir.is_dir() {
                self.count(&mut swept, &entry.file_name().to_string_lossy());
            }
        }
        swept
    }

    /// Removes what a build of unknown outcome may have left of `id`.
    pub(super) fn sweep_sandbox(&self, id: &Uuid7) -> Swept {
        let mut swept = Swept::default();
        if self.config.state_dir.join(id.as_str()).is_dir() {
            self.count(&mut swept, id.as_str());
        }
        swept
    }

    /// Sweeps the sandbox named `name`, logging and counting the outcome.
    fn count(&self, swept: &mut Swept, name: &str) {
        let dir = self.config.state_dir.join(name);
        let sandbox_id = name;
        match sweep_one(&dir, &self.config.cgroup_root.join(name)) {
            Ok(()) => {
                swept.destroyed += 1;
                let event = EVENT_SWEPT;
                tracing::info!(sandbox_id, event);
            }
            Err(error) => {
                swept.left += 1;
                let Logged { error_code, reason } = error.logged();
                let event = EVENT_SWEEP_FAILED;
                tracing::warn!(sandbox_id, error_code, reason, event);
            }
        }
    }
}

/// Kills what still runs, unmounts the disk, removes the directory: the order
/// the parts of a live sandbox are released in.
fn sweep_one(dir: &Path, cgroup: &Path) -> Result<()> {
    if cgroup.is_dir() {
        RunCgroup::leftover(cgroup.to_owned()).remove()?;
    }
    WorkspaceDisk::leftover(dir).release_leftover()?;
    fs::remove_dir_all(dir)?;
    Ok(())
}
