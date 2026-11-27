//! Our Firecracker engine: one microVM per run, its own kernel the wall
//! between the run's tool calls and the host.
//!
//! ```text
//!   prepare(spec) ─► jail: own user, chroot, cgroup leaf, bound namespace
//!                 ─► jailer ─► firecracker --config-file   (no API socket)
//!                       vda toolbox   the verified loop device, read-only
//!                       vdb workspace the run's ext4 image, held by its lock
//!                       tap ─► host table afegress<slot>, reserved ranges dropped
//!                       vsock ─► CONNECT 1024 ─► the guest's agent
//!                 ◄─ once the guest agent answers and the plan reads back in force
//!   park(Stop)    ─► guest sync, monitor stopped, disk kept; resume boots again
//! ```
//!
//! [`FirecrackerBackend`] admits the host at boot — a host whose KVM device a
//! monitor cannot open is refused naming `kvm`, and no other engine starts in
//! its place — and [`FirecrackerEngine`] builds each sandbox. The guest side
//! is not here: `agentsfleet-guest`, a binary of its own, is the guest
//! kernel's `init`, and this crate links none of its crates; it speaks to it
//! through `af_guest_wire`. Linux only: elsewhere the engine states its facts
//! and builds nothing.
//!
//! Only the runner binary and the e2e crate name this crate; every other
//! crate meets it through `afr_engine`'s seam.

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

pub mod error;
pub mod machine;

mod admission;
mod capabilities;
mod config;
mod confinement;
#[cfg_attr(
    not(any(target_os = "linux", test)),
    expect(dead_code, reason = "only a Linux host boots a microVM")
)]
mod console;
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
pub use self::confinement::LANDLOCK_ABI;
pub use self::confinement::{
    LSM_PATH, SECCOMP_ACTIONS_PATH, landlock_enforceable, landlock_listed, seccomp_offered,
};
#[cfg(target_os = "linux")]
pub use self::engine::{FirecrackerBackend, FirecrackerConfig, FirecrackerEngine};
pub use self::error::{Error, MECHANISM_KVM, Result};
pub use self::kvm::{KVM_PATH, admit, probe};
pub use self::lock::ImageLock;
#[cfg(target_os = "linux")]
pub use self::vsock::VsockRoute;
