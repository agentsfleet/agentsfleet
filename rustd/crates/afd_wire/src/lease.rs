//! The lease: the unit of work a runner pulls, and everything it needs to run it.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::event::EventEnvelope;
use crate::memory::MemoryDelta;
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

/// `POST /v1/runners/me/leases` request body: empty.
///
/// It carried a `wire_version` from M157 until Sep 2026. Nothing ever read it —
/// the handler serves one shape unconditionally and never built a body
/// extractor — so the runner spent a field on every poll to tell the daemon
/// something the daemon did not look at. Identity is the Bearer token and the
/// shape is the only shape, which leaves a lease request with nothing to say.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LeaseRequest;

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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
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
