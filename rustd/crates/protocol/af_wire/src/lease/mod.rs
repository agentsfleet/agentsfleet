//! Pushed work and its two authority-separated placement payloads.

mod kinds;
mod limits;
mod payload;

use serde::{Deserialize, Serialize};

pub use kinds::{AcceptedKinds, LeaseKind};
pub use limits::{
    SANDBOX_CPU_MILLIS_MAX, SANDBOX_CPU_MILLIS_MIN, SANDBOX_DISK_BYTES_MAX, SANDBOX_DISK_BYTES_MIN,
    SANDBOX_MEMORY_BYTES_MAX, SANDBOX_MEMORY_BYTES_MIN, SandboxLimits,
};
pub use payload::{
    ANSWER_FAILED, ANSWER_FAILED_END, ANSWER_NONE, HISTORY_BYTES_MAX, HISTORY_TURNS_MAX,
    LeasePayload, SecretDelivery, TURN_TEXT_BYTES_MAX, Turn,
};

/// Placement identifies the execution authority carried by a pushed offer.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaggedLease<'a, C = serde_json::Value> {
    /// Installed fleet work, with its runtime policy.
    Fleet(#[serde(borrow)] Box<LeasePayload<'a>>),
    /// Private setup work, with access to the daemon's scoped model endpoint.
    Authoring(Box<crate::authoring::Lease<C>>),
}
