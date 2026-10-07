//! Sandbox engines: where a lease's tool calls run.
//!
//! [`Engine`] builds one [`Sandbox`] per lease and the sandbox hands back the
//! [`afr_executor::Executor`] running inside it. The interface assumes no file
//! system shared with the host — the workspace is a block image, the toolbox an
//! image file, the executor a Unix socket — so a microVM engine attaches the
//! same artifacts (`docs/architecture/runner_execution.md` §Sandbox engines).
//!
//! # Shape
//!
//! [`bubblewrap`] is the pure command line, [`harden`] what the process inside
//! does to itself, and [`probe`] what the host can enforce. The pieces a lease
//! owns — [`LeaseCgroup`], [`WorkspaceDisk`] — each have one cleanup that
//! consumes them. [`BubblewrapEngine`] composes them on Linux; [`WarmSlots`]
//! wraps any engine; [`UnsandboxedEngine`] serves the executor in-process for
//! tests and refuses to exist in a release build.

pub mod error;

pub mod bubblewrap;
mod cgroup;
mod engine;
mod harden;
mod host;
#[cfg(target_os = "linux")]
mod mounts;
mod probe;
mod serve;
mod tenant;
mod toolbox;
mod unsandboxed;
mod warm_slots;
mod workspace_disk;

#[cfg(target_os = "linux")]
mod bubblewrap_engine;

#[cfg(target_os = "linux")]
pub use self::bubblewrap_engine::{BubblewrapConfig, BubblewrapEngine};
pub use self::cgroup::{
    DEFAULT_IO_BYTES_PER_SECOND, LeaseCgroup, SANDBOX_LEAF, SANDBOX_MEMORY_RESERVE_BYTES,
    SUBTREE_CONTROL, TENANT_LEAF,
};
pub use self::engine::{
    DEFAULT_CPU_MILLIS, DEFAULT_DISK_BYTES, DEFAULT_MEMORY_BYTES, DEFAULT_PIDS, Engine,
    HostWorkspace, Limits, Sandbox, SandboxRequest,
};
pub use self::error::{Error, Result, ToolboxRefusal};
#[cfg(target_os = "linux")]
pub use self::harden::{REFUSED_SYSCALLS, X32_SYSCALL_BIT};
pub use self::harden::{WRITABLE, WRITABLE_DEVICES, capabilities_dropped, harden, single_threaded};
pub use self::host::{HostTools, MKE2FS_PATH, MOUNT_PATH};
pub use self::probe::{
    BWRAP_PATH, CGROUP_ROOT, FILESYSTEMS_PATH, HostProbe, KVM_PATH, Kvm, LSM_PATH,
    MECHANISM_BUBBLEWRAP, MECHANISM_LANDLOCK, MECHANISM_SECCOMP, MECHANISM_TOOLBOX_FILESYSTEM,
    ProbePaths, REQUIRED_CONTROLLERS, SECCOMP_ACTIONS_PATH, probe,
};
pub use self::serve::{serve_confined, serve_sandboxed};
pub use self::tenant::{TENANT_EVENTS_FLAG, TENANT_PROCS_FLAG, TenantDescriptors};
#[cfg(target_os = "linux")]
pub use self::toolbox::KernelMounter;
pub use self::toolbox::{
    Manifest, Mounter, Release, TOOLBOX_KEEP_RELEASES, TOOLBOX_PREFIX, TOOLBOX_RELEASE_PUBLIC_KEY,
    TOOLBOX_SUFFIX, Toolbox, Toolboxes,
};
pub use self::unsandboxed::UnsandboxedEngine;
pub use self::warm_slots::WarmSlots;
pub use self::workspace_disk::{Caching, WorkspaceDisk};
