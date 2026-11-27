//! The confinement a sandbox's tenant processes put on themselves, and the
//! inner environment they run in.
//!
//! [`harden`] is `no_new_privs`, Landlock, seccomp, and a check that no
//! capability survived: the `Hardened` confinement, behind the `Confinement`
//! seam. A microVM's agent, `agentsfleet-guest`, stands outside it as the
//! guest's first process, and its tenant processes run in the inner
//! environment it builds, each confined as the agent chose as it starts
//! (`Namespaced`, Linux only). [`serve_confined`] runs the sequence on a
//! process of its own, so it is proven outside a guest.
//!
//! A guest crate: it links `foundation/` and `protocol/` crates and its own
//! layer's alone, never the runner's.

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

pub mod error;

mod harden;
#[cfg(target_os = "linux")]
mod inner;
mod serve;

pub use self::error::{Error, Result};
#[cfg(target_os = "linux")]
pub use self::harden::{
    Applying, Confinement, Hardened, LANDLOCK_ABI, REFUSED_SYSCALLS, Refused, X32_SYSCALL_BIT,
};
pub use self::harden::{
    DEV_SHM, MECHANISM_LANDLOCK, SANDBOX_TMP, SANDBOX_WORKSPACE, WRITABLE, WRITABLE_DEVICES,
    capabilities_dropped, harden, landlock_enforceable, single_threaded,
};
#[cfg(target_os = "linux")]
pub use self::inner::Namespaced;
pub use self::serve::serve_confined;
