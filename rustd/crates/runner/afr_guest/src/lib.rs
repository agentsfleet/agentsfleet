//! The process inside a sandbox: what it takes from the engine, what it does
//! to itself, and how it serves.
//!
//! [`serve_sandboxed`] adopts the descriptors the engine handed it
//! ([`Inherited`]), [`harden`]s — `no_new_privs`, Landlock, seccomp, and a
//! check that no capability survived — and serves the executor. The engine's
//! half of the handover, the tenant leaf's files and the executor's socket
//! pair, is [`Handover`]. Every engine that runs our executor inside a
//! sandbox serves it this way. A microVM's executor stands outside instead,
//! as the guest's root service, and its tenant processes run in the inner
//! environment it builds, confined the same way (`Namespaced`, Linux only).

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

pub mod error;

mod harden;
#[cfg(target_os = "linux")]
mod inner;
mod probe;
mod serve;
mod tenant;

pub use self::error::{Error, Result};
pub use self::harden::{
    DEV_SHM, SANDBOX_TMP, SANDBOX_WORKSPACE, WRITABLE, WRITABLE_DEVICES, capabilities_dropped,
    harden, landlock_enforceable, single_threaded,
};
#[cfg(target_os = "linux")]
pub use self::harden::{REFUSED_SYSCALLS, X32_SYSCALL_BIT};
#[cfg(target_os = "linux")]
pub use self::inner::Namespaced;
pub use self::probe::{LSM_PATH, SECCOMP_ACTIONS_PATH, landlock_listed, seccomp_offered};
pub use self::serve::{
    Entry, MAX_FRAME_BYTES_FLAG, MAX_READ_BYTES_FLAG, serve_confined, serve_sandboxed,
};
pub use self::tenant::{
    EXECUTOR_FLAG, Inherited, TENANT_EVENTS_FLAG, TENANT_PROCS_FLAG, TenantDescriptors,
};
#[cfg(any(target_os = "linux", test))]
pub use self::tenant::{Handover, TenantFiles};
