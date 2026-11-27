//! The cgroup v2 files both ends of a sandbox write, and the freezer.
//!
//! The runner makes one cgroup per run on the host (`afr_cgroup`), and the
//! in-VM agent makes the tenant leaf inside its guest (`agentsfleet-guest`).
//! Both write the same kernel files, named here once, and both stop a tree
//! with the same [`Freezer`]. Nothing here knows a run, an engine or a guest:
//! only the kernel's interface
//! (<https://docs.kernel.org/admin-guide/cgroup-v2.html>).

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

pub mod error;

mod freezer;

pub use self::error::{Error, Result};
pub use self::freezer::Freezer;

/// Where cgroup v2 is mounted.
pub const CGROUP_ROOT: &str = "/sys/fs/cgroup";
/// The file that enables controllers for a cgroup's children.
pub const SUBTREE_CONTROL: &str = "cgroup.subtree_control";
/// Writing a process identifier moves that process in.
pub const CGROUP_PROCS: &str = "cgroup.procs";
/// Processes and threads a cgroup may hold: the guest's tenant leaf's cap.
pub const PIDS_MAX: &str = "pids.max";
/// What a cgroup's memory limit did, its `oom_kill` count among it.
pub const MEMORY_EVENTS: &str = "memory.events";
/// What a cgroup's process cap did: its `max` count is the forks refused at
/// `pids.max`.
pub const PIDS_EVENTS: &str = "pids.events";
/// The leaf every process the in-VM agent starts runs in.
pub const TENANT_LEAF: &str = "tenant";
