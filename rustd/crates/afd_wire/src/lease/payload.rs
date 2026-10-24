//! Installed fleet work, bounded history and its secret-delivery vocabulary.

use super::SandboxLimits;
use crate::{event::EventEnvelope, policy::ExecutionPolicy};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// How tenant secrets reach the runner.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretDelivery {
    /// Secrets travel in the lease over transport security.
    Inline,
    /// Per-tenant scoped delivery.
    Scoped,
    /// Zero-trust proxied delivery.
    Proxy,
}

/// The most earlier turns a chat lease carries.
pub const HISTORY_TURNS_MAX: usize = 8;
/// The most bytes a chat lease's turns carry, messages and answers together.
pub const HISTORY_BYTES_MAX: usize = 65_536;
/// The most bytes one turn's message, or its answer, carries.
pub const TURN_TEXT_BYTES_MAX: usize = 16_384;
/// A finished run that left no reply.
pub const ANSWER_NONE: &str = "[no reply]";
/// What opens a failed run's answer; the run's failure label follows it.
pub const ANSWER_FAILED: &str = "[the run failed: ";
/// What closes a failed run's answer.
pub const ANSWER_FAILED_END: &str = "]";

/// One earlier exchange in the fleet's thread: what was asked, and what the
/// fleet answered.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Turn<'a> {
    /// The event's message, as `event::message_of` reads it.
    #[serde(borrow)]
    pub message: Cow<'a, str>,
    /// The fleet's answer, or the fixed text a reply-less or failed run reads as.
    #[serde(borrow)]
    pub answer: Cow<'a, str>,
}

/// Work assigned to a runner. Return its `fencing_token` with updates; the server rejects
/// older tokens after reassignment.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeasePayload<'a> {
    /// Identifier for this lease.
    #[serde(borrow)]
    pub lease_id: Cow<'a, str>,
    /// Monotonic guard a report must echo.
    pub fencing_token: u64,
    /// The instant after which the event becomes reclaimable.
    #[serde(with = "crate::instant")]
    pub lease_expires_at: jiff::Timestamp,
    /// How secrets reached this run.
    pub secret_delivery: SecretDelivery,
    /// The event to run.
    #[serde(borrow)]
    pub event: EventEnvelope<'a>,
    /// What the run is permitted to do.
    #[serde(borrow)]
    pub policy: ExecutionPolicy<'a>,
    /// The installed fleet's behaviour prose, so the sandboxed turn runs the
    /// installed behaviour rather than a generic one. Soft reasoning input —
    /// hard tool and secret policy stays in `policy`.
    #[serde(borrow)]
    pub instructions: Cow<'a, str>,
    /// The sandbox to run in; null uses this runner's configured defaults.
    pub limits: Option<SandboxLimits>,
    /// The fleet's earlier turns, oldest first, ahead of this event; empty
    /// unless the event is a chat message.
    #[serde(borrow)]
    pub history: Vec<Turn<'a>>,
    /// True when the runner can reuse its latest held sandbox for a new event. Reassigned
    /// events and expired or outdated sandboxes require a fresh sandbox.
    pub resume_hold: bool,
}
