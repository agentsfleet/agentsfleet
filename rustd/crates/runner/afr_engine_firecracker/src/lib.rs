//! Our Firecracker engine: one microVM per run, its own kernel the wall
//! between the run's tool calls and the host.
//!
//! ```text
//!   prepare(spec) ─► jail: own user, chroot, cgroup leaf, bound namespace
//!                 ─► jailer ─► firecracker --config-file   (no API socket)
//!                       vda toolbox   the verified loop device, read-only
//!                       vdb workspace the run's ext4 image, held by its lock
//!                       tap ─► host table afegress<slot>, reserved ranges dropped
//!                       vsock ─► CONNECT 1024 ─► the guest's executor
//!                 ◄─ once the executor answers and the plan reads back in force
//!   park(Stop)    ─► guest sync, monitor stopped, disk kept; resume boots again
//! ```
//!
//! [`FirecrackerBackend`] admits the host at boot — a host whose KVM device a
//! monitor cannot open is refused naming `kvm`, and no other engine starts in
//! its place — and [`FirecrackerEngine`] builds each sandbox. The guest side,
//! the runner binary as the guest kernel's `init`, is [`guest`]. Linux only:
//! elsewhere the engine states its facts and builds nothing.
//!
//! Only the runner binary and the e2e crate name this crate; every other
//! crate meets it through `afr_engine`'s seam.

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

pub mod error;
pub mod guest;
pub mod machine;

mod admission;
mod capabilities;
mod config;
#[cfg(target_os = "linux")]
mod engine;
#[cfg(target_os = "linux")]
mod jail;
mod kvm;
mod lock;
mod sweep;
#[cfg(target_os = "linux")]
mod vsock;

pub use self::admission::{Admission, Reservation};
pub use self::capabilities::{CAPABILITIES as FIRECRACKER_CAPABILITIES, MAX_CPU_MILLIS, grant};
pub use self::config::{
    Binaries, DISK_OVERCOMMIT, FIRECRACKER_PATH, FIRST_JAIL_ID, FirecrackerTuning, INITRD_PATH,
    JAILER_PATH, KERNEL_PATH, MEMORY_OVERCOMMIT, MONITOR_OVERHEAD_BYTES, WRITE_CACHE_BYTES,
};
#[cfg(target_os = "linux")]
pub use self::engine::{FirecrackerBackend, FirecrackerConfig, FirecrackerEngine};
pub use self::error::{Error, MECHANISM_KVM, Result};
pub use self::kvm::{KVM_PATH, admit, probe};
pub use self::lock::ImageLock;
#[cfg(target_os = "linux")]
pub use self::vsock::VsockRoute;
