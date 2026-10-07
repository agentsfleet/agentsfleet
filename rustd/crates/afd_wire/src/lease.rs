//! The lease: the unit of work a runner pulls, and everything it needs to run it.

use std::borrow::Cow;

use garde::Validate;
use serde::{Deserialize, Serialize};

use crate::event::EventEnvelope;
use crate::policy::ExecutionPolicy;
use crate::runner::HeldFleets;

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

/// Content-addressed reference to an installed Fleet Bundle's snapshot.
///
/// The hash's presence on a lease IS the "has bundle" signal. A `404` from the
/// download means the bundle is skill-only and the runner proceeds with none.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleManifest<'a> {
    /// Content hash addressing the immutable canonical archive.
    #[serde(borrow)]
    pub content_hash: Cow<'a, str>,
}

/// The smallest processor share a lease may ask for, in thousandths of a core.
pub const SANDBOX_CPU_MILLIS_MIN: u32 = 250;
/// The largest processor share a lease may ask for: 32 cores.
pub const SANDBOX_CPU_MILLIS_MAX: u32 = 32_000;
/// The least memory a lease may ask for. Above the sandbox's own reserve, so
/// the tenant's processes always get some.
pub const SANDBOX_MEMORY_BYTES_MIN: u64 = 256 * 1024 * 1024;
/// The most memory a lease may ask for: 64 GiB.
pub const SANDBOX_MEMORY_BYTES_MAX: u64 = 64 * 1024 * 1024 * 1024;
/// The smallest workspace disk a lease may ask for: 1 GiB.
pub const SANDBOX_DISK_BYTES_MIN: u64 = 1024 * 1024 * 1024;
/// The largest workspace disk a lease may ask for: 256 GiB.
pub const SANDBOX_DISK_BYTES_MAX: u64 = 256 * 1024 * 1024 * 1024;

/// The sandbox a lease asks for.
///
/// Bounded, because the runner builds exactly what it is told. A size past
/// these bounds is a daemon fault, so the runner refuses the lease. Whether
/// this host has room for it is a separate question.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct SandboxLimits {
    /// Processor share, in thousandths of one core.
    #[garde(range(min = SANDBOX_CPU_MILLIS_MIN, max = SANDBOX_CPU_MILLIS_MAX))]
    // utoipa takes only literals here; `validation_lease.rs` pins them to the
    // constants garde reads.
    #[cfg_attr(feature = "openapi", schema(minimum = 250, maximum = 32_000))]
    pub cpu_millis: u32,
    /// Memory, in bytes.
    #[garde(range(min = SANDBOX_MEMORY_BYTES_MIN, max = SANDBOX_MEMORY_BYTES_MAX))]
    #[cfg_attr(feature = "openapi", schema(minimum = 268_435_456_u64))]
    #[cfg_attr(feature = "openapi", schema(maximum = 68_719_476_736_u64))]
    pub memory_bytes: u64,
    /// The workspace disk's size, in bytes.
    #[garde(range(min = SANDBOX_DISK_BYTES_MIN, max = SANDBOX_DISK_BYTES_MAX))]
    #[cfg_attr(feature = "openapi", schema(minimum = 1_073_741_824_u64))]
    #[cfg_attr(feature = "openapi", schema(maximum = 274_877_906_944_u64))]
    pub disk_bytes: u64,
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
    /// The sandbox to run in; null means the runner's own defaults. Absent
    /// decodes as null, so a daemon that predates the field still leases.
    #[serde(default)]
    pub limits: Option<SandboxLimits>,
    /// The fleet's earlier turns, oldest first, ahead of this event; empty
    /// unless the event is a chat message. Absent decodes as empty.
    #[serde(default, borrow)]
    pub history: Vec<Turn<'a>>,
    /// True when the sandbox this runner holds for the fleet is the fleet's
    /// latest and this event has not run before: the slot's last lease ran
    /// here, its hold had not lapsed at the claim, and the event is not a
    /// reclaim. Anything else builds a fresh sandbox, because the held one may
    /// predate another runner's run or carry this event's own first attempt.
    /// Absent decodes as false.
    #[serde(default)]
    pub resume_hold: bool,
}

/// `POST /v1/runners/me/leases` request.
///
/// What a polling runner tells the daemon about itself: the fleets whose
/// sandboxes it holds, so a held fleet's next event reaches it first. An empty
/// or unreadable body reads as holding nothing, so a poll never fails over
/// what it carries.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct LeaseRequest<'a> {
    /// Every fleet this runner holds a frozen sandbox for. Absent decodes as
    /// empty.
    #[serde(borrow, default)]
    #[garde(dive)]
    #[cfg_attr(
        feature = "openapi",
        schema(value_type = Vec<String>, max_items = 64, min_length = 36, max_length = 36)
    )]
    pub holds: HeldFleets<'a>,
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
