//! The bubblewrap engine: one hardened sandbox per run on the host's kernel.
//!
//! [`bubblewrap`] is the pure command line one run's sandbox starts with;
//! [`BubblewrapEngine`] composes the building blocks around it on Linux — the
//! run's cgroup ([`afr_cgroup`]), its workspace disk and the toolbox
//! ([`afr_image`]), its egress scope ([`afr_firewall`]) and the sandbox entry
//! that hardens and serves inside ([`afr_guest`]). [`probe`] assembles what
//! the host can enforce from each building block's own probe, and
//! [`BubblewrapBackend`] admits the host at boot.
//!
//! Only the runner binary and the e2e crate name this crate; every other
//! crate meets it through `afr_engine`'s seam.

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

#[cfg(target_os = "linux")]
pub mod error;

pub mod bubblewrap;
#[cfg(target_os = "linux")]
mod bubblewrap_engine;
mod probe;
mod tuning;

pub use self::bubblewrap::CAPABILITIES as BUBBLEWRAP_CAPABILITIES;
pub use self::bubblewrap::Requirements as BubblewrapRequirements;
#[cfg(target_os = "linux")]
pub use self::bubblewrap_engine::{
    BubblewrapBackend, BubblewrapConfig, BubblewrapEngine, SANDBOX_HOST_IDS,
};
#[cfg(target_os = "linux")]
pub use self::error::{Error, Result};
pub use self::probe::{KVM_PATH, ProbePaths, measure, probe};
pub use self::tuning::EngineTuning;
