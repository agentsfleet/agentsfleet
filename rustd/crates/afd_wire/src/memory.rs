//! Durable fleet memory: the hydrate and capture halves, and their byte budgets.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// Total memory bytes one push may carry, summed over every delta.
///
/// The runner caps what it surfaces and the control plane rejects beyond this.
/// Oversized memory is truncated and logged, never silently dropped whole.
pub const MAX_PUSH_BYTES: usize = 256 * 1024;

/// Ceiling on durable entries one fleet may accumulate across all its runs.
///
/// A backstop, not the primary bound — stable-key overwrite and explicit
/// forgetting are the fleet's own. Eviction beyond this is tier-ordered.
pub const MAX_ENTRIES_PER_FLEET: usize = 1000;

/// Byte budget for one hydration window.
///
/// Bounds the payload a run seeds into the child regardless of how large the
/// durable set has grown; dropped entries stay durable, just unhydrated.
pub const HYDRATE_WINDOW_BYTES: usize = 256 * 1024;

/// One durable memory item — the unit of both capture and hydrate.
///
/// Carries no scope: the fleet is a path segment, validated server-side against
/// the runner's live lease.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryDelta<'a> {
    /// Stable key. A repeated key overwrites rather than accumulating.
    #[serde(borrow)]
    pub key: Cow<'a, str>,
    /// The remembered content.
    #[serde(borrow)]
    pub content: Cow<'a, str>,
    /// Retention category, which decides eviction order.
    #[serde(borrow)]
    pub category: Cow<'a, str>,
}

/// `POST /v1/runners/me/memory/{fleet_id}` request.
///
/// The lease and fencing token ride the body exactly as they do on a report: the
/// control plane loads that lease, verifies the runner owns it, cross-checks the
/// fleet against the path, and fences the write. Each delta is upserted, so a
/// retried push is idempotent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryPushRequest<'a> {
    /// The lease authorizing this write.
    #[serde(borrow)]
    pub lease_id: Cow<'a, str>,
    /// Monotonic guard; a reclaimed holder is rejected.
    pub fencing_token: u64,
    /// The items to remember.
    #[serde(borrow)]
    pub memory: Vec<MemoryDelta<'a>>,
}

/// `GET /v1/runners/me/memory/{fleet_id}` reply — a compacted hydration window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryHydrateResponse<'a> {
    /// The window's items.
    #[serde(borrow)]
    pub memory: Vec<MemoryDelta<'a>>,
}
