//! The agent as a Firecracker microVM's first process: what it proves, what
//! it mounts, and the vsock connection it serves.
//!
//! `agentsfleet-guest` is a thin binary over this crate. It refuses unless it
//! is the guest kernel's process 1 with a vsock device ([`admit`]); then it
//! mounts the guest's drives, builds the tenant leaf and the inner
//! environment, and serves `afg_agent` to the host. Its death ends the
//! microVM, and the run resumes from its checkpoint on a fresh one.
//!
//! A guest crate: it links `foundation/` and `protocol/` crates and its own
//! layer's alone, never the runner's. Linux only past the admission check.

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

pub mod error;

mod admit;
mod vm;

use af_guest_wire::GuestEntry;

pub use self::admit::{Booted, Machine, admit};
pub use self::error::{Error, Result};

/// Proves `machine` is a microVM's first process, then mounts the guest and
/// serves the host until the agent ends, and powers the guest off.
///
/// # Errors
/// The process is not one the agent runs as, a file system would not mount,
/// or the agent could not start; the kernel then ends the guest, since its
/// `init` returned.
pub fn run(entry: GuestEntry, machine: &impl Machine) -> Result<()> {
    admit(machine)?;
    serve(entry)
}

#[cfg(target_os = "linux")]
fn serve(entry: GuestEntry) -> Result<()> {
    vm::serve_vm(entry)
}

/// Only a Linux guest kernel starts the agent; [`admit`] already refused.
#[cfg(not(target_os = "linux"))]
fn serve(_entry: GuestEntry) -> Result<()> {
    Err(error::not_init(
        "only a Linux guest kernel starts the agent".to_owned(),
    ))
}
