//! The lease: the unit of work a runner pulls, and everything it needs to run it.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::event::EventEnvelope;
use crate::memory::MemoryDelta;
use crate::paths::LEASE_WIRE_VERSION_CURRENT;
use crate::policy::ExecutionPolicy;

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

/// `POST /v1/runners/me/leases` request body.
///
/// Defaults to the current version, which is the only version this port serves.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseRequest {
    /// The wire version the caller speaks.
    pub wire_version: u16,
}

impl Default for LeaseRequest {
    fn default() -> Self {
        Self {
            wire_version: LEASE_WIRE_VERSION_CURRENT,
        }
    }
}

/// Content-addressed reference to an installed Fleet Bundle's snapshot.
///
/// The hash's presence on a lease IS the "has bundle" signal. A `404` from the
/// download means the bundle is skill-only and the runner proceeds with none.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BundleManifest<'a> {
    /// Content hash addressing the immutable canonical archive.
    #[serde(borrow)]
    pub content_hash: Cow<'a, str>,
}

/// The work half of a lease.
///
/// `fencing_token` is a monotonic guard: a report must echo it, and a stale
/// holder carrying an older token is rejected. That is what makes reporting safe
/// under lease reclaim, beyond plain idempotency by event id.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeasePayload<'a> {
    /// Identifier for this lease.
    #[serde(borrow)]
    pub lease_id: Cow<'a, str>,
    /// Monotonic guard a report must echo.
    pub fencing_token: u64,
    /// Epoch milliseconds after which the event becomes reclaimable.
    pub lease_expires_at: i64,
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
    /// The bundle to materialize, when the fleet was created from one.
    #[serde(borrow)]
    pub bundle: Option<BundleManifest<'a>>,
}

/// `POST /v1/runners/me/leases` reply. Always `200`.
///
/// `lease` is the work, or null with `retry_after_ms` set when there is none —
/// a backoff hint rather than a `204`.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LeaseResponse<'a> {
    /// The work, when there is any.
    #[serde(borrow)]
    pub lease: Option<LeasePayload<'a>>,
    /// How long to wait before asking again, when there is none.
    pub retry_after_ms: Option<u32>,
}

/// What the runner parent pipes to the sandboxed child's standard input.
///
/// The parent hydrated the memory over the trusted plane because it holds the
/// token; the child makes no network call of its own, so no credential, URL or
/// connection string ever reaches the sandboxed fleet.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerChildInput<'a> {
    /// The lease to execute.
    #[serde(borrow)]
    pub lease: LeasePayload<'a>,
    /// The fleet's prior memory, already hydrated by the parent.
    #[serde(borrow)]
    pub hydrated_memory: Vec<MemoryDelta<'a>>,
}
