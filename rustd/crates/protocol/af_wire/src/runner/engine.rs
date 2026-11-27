//! What builds a runner's sandboxes, as the runner reports it and the engine
//! seam names it: one spelling on the wire and in the runner.

use serde::{Deserialize, Serialize};

/// The engine that builds a runner's sandboxes.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, strum::IntoStaticStr)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum SandboxEngine {
    /// A Firecracker microVM with its own kernel.
    Firecracker,
    /// The runner's own processes on the host's file system, with a scratch
    /// workspace and no confinement. Debug builds only.
    Unsandboxed,
}

/// The wall between a sandbox's code and the runner host.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, strum::IntoStaticStr)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum Isolation {
    /// None: the code runs as the runner's own processes.
    Unconfined,
    /// A virtual machine with its own kernel, behind the processor's
    /// virtualization.
    HardwareVm,
}

/// What the runner host's virtualization device offers.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, strum::IntoStaticStr)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum Kvm {
    /// `/dev/kvm` exists and the runner opened it.
    Usable,
    /// `/dev/kvm` exists and opening it was refused.
    Denied,
    /// The host has no `/dev/kvm`.
    Absent,
}
