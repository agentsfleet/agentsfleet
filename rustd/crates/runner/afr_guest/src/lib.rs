//! The confinement a sandbox's tenant processes put on themselves, and the
//! inner environment they run in.
//!
//! [`harden`] is `no_new_privs`, Landlock, seccomp, and a check that no
//! capability survived. A microVM's executor stands outside it, as the
//! guest's root service, and its tenant processes run in the inner
//! environment it builds, each confined that way as it starts (`Namespaced`,
//! Linux only). [`serve_confined`] runs the sequence on a process of its own,
//! so it is proven outside a guest.

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

pub mod error;

mod harden;
#[cfg(target_os = "linux")]
mod inner;
mod probe;
mod serve;

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
pub use self::serve::serve_confined;
