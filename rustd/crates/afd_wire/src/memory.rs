//! Durable fleet memory: the hydrate and capture halves, and their byte budgets.

use std::borrow::Cow;

use garde::Validate;
use serde::{Deserialize, Serialize};

/// Longest stored key, in bytes.
///
/// The operator surface bounds a path segment by it before decoding one: a key
/// too long to have been stored cannot name a row.
pub const MAX_KEY_LEN: usize = 255;

/// Longest stored content, in bytes.
pub const MAX_CONTENT_LEN: usize = 16 * 1024;

/// Longest stored category, in bytes. The column carries no CHECK, so this is
/// the only bound on a category label.
pub const MAX_CATEGORY_LEN: usize = 64;

/// The one category that hydrates before recency is considered, and that
/// eviction protects. It is also the category a runner stores under when the
/// model names none.
pub const PINNED_CATEGORY: &str = "core";

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
// the runner's live lease. The bounds are declared here, so the daemon's push
// and the runner's store refuse the same entries.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct MemoryDelta<'a> {
    /// Stable key. A repeated key overwrites rather than accumulating.
    #[serde(borrow)]
    #[garde(length(bytes, min = 1, max = MAX_KEY_LEN))]
    pub key: Cow<'a, str>,
    /// The remembered content.
    #[serde(borrow)]
    #[garde(length(bytes, min = 1, max = MAX_CONTENT_LEN))]
    pub content: Cow<'a, str>,
    /// Retention category, which decides eviction order.
    #[serde(borrow)]
    #[garde(length(bytes, min = 1, max = MAX_CATEGORY_LEN))]
    pub category: Cow<'a, str>,
}

impl MemoryDelta<'_> {
    /// The bytes this entry charges against a memory budget: the hydration
    /// window, the push cap and the dropped-bytes count all charge the same.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.key.len() + self.content.len() + self.category.len()
    }

    /// This entry as a view that borrows its text from `self`.
    #[must_use]
    pub fn view(&self) -> MemoryDelta<'_> {
        MemoryDelta {
            key: Cow::Borrowed(&self.key),
            content: Cow::Borrowed(&self.content),
            category: Cow::Borrowed(&self.category),
        }
    }

    /// This entry detached from what it borrowed: owned text moves, and only
    /// borrowed text is copied.
    #[must_use]
    pub fn into_owned(self) -> MemoryDelta<'static> {
        MemoryDelta {
            key: Cow::Owned(self.key.into_owned()),
            content: Cow::Owned(self.content.into_owned()),
            category: Cow::Owned(self.category.into_owned()),
        }
    }
}

/// `POST /v1/runners/me/memory/{fleet_id}` request.
//
// The lease and fencing token ride the body exactly as they do on a report: the
// control plane loads that lease, verifies the runner owns it, cross-checks the
// fleet against the path, and fences the write. Each delta is upserted, so a
// retried push is idempotent.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
// used to live. What the inline version claimed was that a response body could
// be spelled somewhere other than this crate, and two keys written by hand at a
// call site are two keys nothing type-checks.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    // The name is the one that shipped, and `handler.zig` answers the page
    // length too.
    /// How many memories this page carries. The count covers this page only,
    /// not the whole Fleet.
    pub total: usize,
    /// Where the next page resumes, or `null` on the last one.
    #[serde(borrow)]
    pub next_cursor: Option<Cow<'a, str>>,
}
