//! One cgroup v2 per sandbox, and the delegated root every one is made under.
//!
//! [`RunCgroup`] holds a run's limits and its two leaves; its one cleanup
//! consumes it, and its freezer (`af_cgroup::Freezer`) parks and resumes
//! its tree. [`delegated_root`] finds the subtree the service manager handed
//! this runner. The kernel file names this crate shares with the in-VM agent
//! live in `af_cgroup`.

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

pub mod error;

mod cgroup;

pub use self::cgroup::{
    DEFAULT_IO_BYTES_PER_SECOND, MECHANISM_DELEGATED_CGROUP, Room, RunCgroup, SANDBOX_LEAF,
    SELF_CGROUP_PATH, delegated_controllers, delegated_root, enable_controllers,
};
pub use self::error::{Error, Result};
