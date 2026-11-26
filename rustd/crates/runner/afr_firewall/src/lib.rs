//! The host's egress table: what joins an `allow_list_egress` sandbox's
//! network namespace to the host, and holds it to its allowlist.
//!
//! A [`HostScope`] is one sandbox's veth pair and `nf_tables` table, built
//! over netlink from this process; [`own_host`] takes the lock one runner
//! process owns the host's egress objects under, and [`sweep`] removes what a
//! previous one left. Linux only: no other kernel has `nf_tables`, so
//! elsewhere [`enforceable`] answers no and nothing else exists.

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

pub mod error;

#[cfg(target_os = "linux")]
mod egress;

/// What the kernel lane proves egress against: the egress objects in the
/// host's namespace, a killed run's leftovers, and a host past the sandbox's
/// link (M-TEST-UTIL).
#[cfg(all(target_os = "linux", feature = "test-util"))]
pub use self::egress::far;
#[cfg(target_os = "linux")]
pub use self::egress::{HostScope, enforceable, namespace_of, own_host, sweep};
pub use self::error::{Error, Result};

/// No other kernel has `nf_tables`, so no host here holds a sandbox to an
/// allowlist.
#[cfg(not(target_os = "linux"))]
#[must_use]
pub const fn enforceable() -> bool {
    false
}
