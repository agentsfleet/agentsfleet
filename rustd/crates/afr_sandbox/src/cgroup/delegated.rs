//! The cgroup a service manager delegated to this process: where every lease's
//! cgroup is made.
//!
//! systemd's `Delegate=` with `DelegateSubgroup=` starts the runner in a leaf
//! of its own and hands it the service's cgroup to manage
//! (`deploy/baremetal/agentsfleet-runner.service`). cgroup v2 keeps processes
//! out of an inner node, so a lease's cgroup is a sibling of that leaf, made
//! under its parent: the delegated root is the parent of the cgroup
//! `/proc/self/cgroup` names.

use std::io::{self, BufRead};
use std::path::{Path, PathBuf};

use procfs_core::FromBufRead as _;
use procfs_core::ProcessCGroups;

use crate::error::{Result, refused};

/// Where the kernel names the cgroups this process runs in.
pub const SELF_CGROUP_PATH: &str = "/proc/self/cgroup";
/// The mechanism a process outside any delegated cgroup lacks, as a refusal
/// names it.
pub const MECHANISM_DELEGATED_CGROUP: &str = "delegated_cgroup";
/// The unified (v2) hierarchy's number in `/proc/self/cgroup`, the only one
/// whose line carries no controller list.
const UNIFIED_HIERARCHY: u32 = 0;

/// The cgroup delegated to this process, under the cgroup v2 mount `mount`,
/// read from `own`, the text of [`SELF_CGROUP_PATH`].
///
/// # Errors
/// `own` does not parse, names no unified-hierarchy cgroup, or names one at the
/// top of the tree, whose parent would be the whole host.
pub fn delegated_root(own: impl BufRead, mount: &Path) -> Result<PathBuf> {
    let groups = ProcessCGroups::from_buf_read(own).map_err(io::Error::other)?;
    groups
        .0
        .into_iter()
        .find(|group| group.hierarchy == UNIFIED_HIERARCHY && group.controllers.is_empty())
        .and_then(|unified| {
            Path::new(&unified.pathname)
                .parent()
                .and_then(|parent| parent.strip_prefix("/").ok())
                .filter(|parent| !parent.as_os_str().is_empty())
                .map(|parent| mount.join(parent))
        })
        .ok_or_else(|| refused(MECHANISM_DELEGATED_CGROUP))
}

#[cfg(test)]
#[path = "delegated_tests.rs"]
mod tests;
