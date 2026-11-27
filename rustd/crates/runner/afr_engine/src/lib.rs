//! The seam every sandbox engine meets: where a run's tool calls run.
//!
//! [`Engine`] builds one [`Sandbox`] per run and the sandbox hands back the
//! [`afr_executor::Executor`] running inside it. The interface assumes no file
//! system shared with the host — the workspace is a block image, the toolbox an
//! image file, the executor a byte stream — so a microVM engine attaches the
//! same artifacts (`docs/architecture/runner_execution.md` §Sandbox engines).
//!
//! # Shape
//!
//! The traits and the facts an engine states, the egress plan every engine
//! enforces through [`enforce`], boot admission ([`Backend`], [`boot`]) and
//! [`WarmSlots`], which wraps any engine. Nothing here calls the operating
//! system: each engine lives in a crate of its own, over the building blocks
//! it composes, and only the runner binary names one. The supervisor depends
//! on this crate alone.

#![cfg_attr(not(test), deny(unused_crate_dependencies))]
#![deny(clippy::self_named_module_files)]

#[cfg(any(test, feature = "test-util"))]
pub mod conformance;
pub mod error;

mod boot;
mod engine;
mod network;
mod probe;
mod warm_slots;

pub use self::boot::{Backend, Booted, HostPaths, Resources, Started, boot};
#[cfg(any(test, feature = "test-util"))]
pub use self::engine::fresh_sandbox_id;
pub use self::engine::volume::{Paused, Resumed, WorkspaceVolume, pin_paused};
pub use self::engine::{
    AdmissionBound, Cap, Capabilities, DEFAULT_CPU_MILLIS, DEFAULT_DISK_BYTES,
    DEFAULT_MEMORY_BYTES, DEFAULT_PIDS, DEFAULT_READY_TIMEOUT, EgressPlan, Enforcer, Engine,
    Environment, Generation, Granted, Killed, Limits, Overcommitted, ParkMode, Parked, Quiescence,
    Resource, ResourceFacts, Root, Sandbox, SandboxSpec, SandboxState, SnapshotRef, Snapshots,
    Sweep, Swept, enforce, until_in_force,
};
#[cfg(feature = "test-util")]
pub use self::error::ambiguous;
pub use self::error::{EgressRefusal, Error, Failure, Result};
pub use self::network::{
    Allowlist, Network, NetworkKind, RESOLV_CONF, SANDBOX_HOSTS, SANDBOX_RESOLV_CONF,
};
pub use self::probe::{
    HostProbe, MECHANISM_LANDLOCK, MECHANISM_NFTABLES, MECHANISM_SECCOMP,
    MECHANISM_TOOLBOX_FILESYSTEM, REQUIRED_CONTROLLERS,
};
pub use self::warm_slots::WarmSlots;
