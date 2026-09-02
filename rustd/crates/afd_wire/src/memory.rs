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

/// One durable memory item — the unit of both reading and writing.
//
// Carries no scope: the fleet is a path segment, validated server-side against
// the runner's live lease.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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
//
// The lease and fencing token ride the body exactly as they do on a report: the
// control plane loads that lease, verifies the runner owns it, cross-checks the
// fleet against the path, and fences the write. Each delta is upserted, so a
// retried push is idempotent.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
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

/// What a fleet remembers, compacted to fit one window.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryHydrateResponse<'a> {
    /// The window's items.
    #[serde(borrow)]
    pub memory: Vec<MemoryDelta<'a>>,
}

/// `POST /v1/runners/me/memory/{fleet_id}` reply — what the write did.
//
// A runner acts on both numbers: `stored` says its memory landed, `skipped`
// says some was refused for shape and it should look at what it sends. The
// sweep and eviction counts the control plane also computes stay in the log —
// they are the daemon's housekeeping, not a fact about this request.
//
// Declared here rather than assembled inline at the handler, which is where it
// used to live. The argument for inline was that no `wire-v2` fixture pins
// this shape, so a type would claim a frozen contract the corpus does not
// carry. That confuses two things: `tests/roundtrip.rs` generates its cases
// from an explicit fixture ROSTER, so a type absent from that roster is
// pinned by nothing and claims nothing. What the inline version did claim was
// that a response body could be spelled somewhere other than this crate, and
// two keys written by hand at a call site are two keys nothing type-checks.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryCaptureResponse {
    /// Deltas written, after upsert.
    pub stored: usize,
    /// Deltas refused for shape, which the runner should investigate.
    pub skipped: usize,
}

/// One stored entry as the OPERATOR surface renders it.
//
// A [`MemoryDelta`] plus the instant it was last written. The runner's two
// verbs carry no timestamp — a fleet being seeded with what it knows has no
// use for one — while a person reading the list is deciding whether a lesson
// is still current, which is the whole question `updated_at` answers.
//
// Field order is load-bearing. `memory/handler.zig` hands its `MemoryEntry`
// straight to `res.json`, which emits the struct's fields in DECLARATION
// order, and a dashboard diffing two responses byte-for-byte would see a
// reorder as a change.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryEntry<'a> {
    /// The stable key the fleet remembers this under.
    #[serde(borrow)]
    pub key: Cow<'a, str>,
    /// What it remembers.
    #[serde(borrow)]
    pub content: Cow<'a, str>,
    /// The retention category, which decides eviction order.
    #[serde(borrow)]
    pub category: Cow<'a, str>,
    /// Epoch milliseconds, as a JSON NUMBER — never a decimal string.
    pub updated_at: i64,
}

/// `GET /v1/workspaces/{workspace_id}/fleets/{fleet_id}/memories` — one page.
//
// Exactly three fields, and an integration test pins the count: a page that
// grew a fourth would be a shape the dashboard's parser did not agree to.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoriesResponse<'a> {
    /// The entries on this page, newest first.
    #[serde(borrow)]
    pub items: Vec<MemoryEntry<'a>>,
    /// How many are on THIS page — not the fleet's whole count. The name is
    /// the one that shipped, and `handler.zig` answers the page length too.
    pub total: usize,
    /// Where the next page resumes, or `null` on the last one.
    #[serde(borrow)]
    pub next_cursor: Option<Cow<'a, str>>,
}
