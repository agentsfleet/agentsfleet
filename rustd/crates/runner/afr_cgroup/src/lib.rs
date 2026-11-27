//! One cgroup v2 per sandbox, and the delegated root every one is made under.
//!
//! [`RunCgroup`] holds a run's limits and its two leaves; its one cleanup
//! consumes it. [`Freezer`] parks and resumes its tree, and
//! [`delegated_root`] finds the subtree the service manager handed this
//! runner. The engine opens the tenant leaf's files for the sandbox entry to
//! attribute its processes by (`afr_guest`).

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

pub mod error;

mod cgroup;

pub use self::cgroup::{
    CGROUP_PROCS, CGROUP_ROOT, DEFAULT_IO_BYTES_PER_SECOND, Freezer, MECHANISM_DELEGATED_CGROUP,
    MEMORY_EVENTS, RunCgroup, SANDBOX_LEAF, SANDBOX_MEMORY_RESERVE_BYTES, SELF_CGROUP_PATH,
    SUBTREE_CONTROL, TENANT_LEAF, WRITEBACK_HEADROOM_BYTES, delegated_controllers, delegated_root,
    enable_controllers,
};
pub use self::error::{Error, Result};
