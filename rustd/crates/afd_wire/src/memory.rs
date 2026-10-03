//! Durable fleet memory: the hydrate and capture halves, and their byte budgets.

use std::borrow::Cow;

use afd_validate::nul_free;
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

/// Byte budget for the workspace's shared entries one hydrate carries.
///
/// A quarter of the fleet's own window, spent after it: what other fleets
/// published informs a run and never crowds out what the fleet itself learned.
pub const HYDRATE_SHARED_BYTES: usize = HYDRATE_WINDOW_BYTES / 4;

/// The most entries of each kind one recall answers with.
pub const RECALL_LIMIT_MAX: usize = 50;

/// Who reads a stored entry: its own fleet, or every fleet in the workspace
/// granted to read shared memory.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    /// Only the fleet that wrote it.
    #[default]
    Fleet,
    /// Every fleet in the workspace holding the read grant.
    Workspace,
}

impl Visibility {
    /// Whether only the writing fleet reads the entry. Takes `&self` because
    /// serde's `skip_serializing_if` hands it a reference.
    #[must_use]
    pub const fn is_fleet(&self) -> bool {
        matches!(self, Self::Fleet)
    }

    /// Whether every granted fleet in the workspace reads the entry.
    #[must_use]
    pub const fn is_workspace(self) -> bool {
        matches!(self, Self::Workspace)
    }
}

/// One durable memory item — the unit of both reading and writing.
//
// Carries no scope: the fleet is a path segment, validated server-side against
// the runner's live lease. The bounds are declared here, so the daemon's push
// and the runner's store refuse the same entries. NUL is refused on every text
// field because Postgres cannot store it in `text`: one such delta would fail
// the whole push's statement instead of being skipped as malformed.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
pub struct MemoryDelta<'a> {
    /// Stable key. A repeated key overwrites rather than accumulating.
    #[serde(borrow)]
    #[garde(length(bytes, min = 1, max = MAX_KEY_LEN), custom(nul_free))]
    pub key: Cow<'a, str>,
    /// The remembered content.
    #[serde(borrow)]
    #[garde(length(bytes, min = 1, max = MAX_CONTENT_LEN), custom(nul_free))]
    pub content: Cow<'a, str>,
    /// Retention category, which decides eviction order.
    #[serde(borrow)]
    #[garde(length(bytes, min = 1, max = MAX_CATEGORY_LEN), custom(nul_free))]
    pub category: Cow<'a, str>,
    /// Who reads it; absent means the writing fleet alone. Left off the wire
    /// at that default, so a delta that shares nothing reads as it always did.
    #[serde(default, skip_serializing_if = "Visibility::is_fleet")]
    #[garde(skip)]
    pub visibility: Visibility,
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
            visibility: self.visibility,
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
            visibility: self.visibility,
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

/// An entry another fleet in the workspace published, and who wrote it.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedMemory<'a> {
    /// The key its writer stored it under.
    #[serde(borrow)]
    pub key: Cow<'a, str>,
    /// What its writer remembered.
    #[serde(borrow)]
    pub content: Cow<'a, str>,
    /// The writer's retention category.
    #[serde(borrow)]
    pub category: Cow<'a, str>,
    /// The fleet that wrote it, and the only fleet that may change it.
    #[serde(borrow)]
    pub writer_fleet_id: Cow<'a, str>,
    /// That fleet's name in the workspace.
    #[serde(borrow)]
    pub writer_fleet_name: Cow<'a, str>,
    /// Epoch milliseconds of its last write.
    pub updated_at: i64,
}

impl SharedMemory<'_> {
    /// The bytes this entry charges against the shared hydration budget.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.key.len() + self.content.len() + self.category.len() + self.writer_fleet_name.len()
    }

    /// This entry detached from what it borrowed: owned text moves, and only
    /// borrowed text is copied.
    #[must_use]
    pub fn into_owned(self) -> SharedMemory<'static> {
        SharedMemory {
            key: Cow::Owned(self.key.into_owned()),
            content: Cow::Owned(self.content.into_owned()),
            category: Cow::Owned(self.category.into_owned()),
            writer_fleet_id: Cow::Owned(self.writer_fleet_id.into_owned()),
            writer_fleet_name: Cow::Owned(self.writer_fleet_name.into_owned()),
            updated_at: self.updated_at,
        }
    }
}

/// What a fleet remembers, compacted to fit one window.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryHydrateResponse<'a> {
    /// The window's items.
    #[serde(borrow)]
    pub memory: Vec<MemoryDelta<'a>>,
    /// What other fleets in the workspace published, newest first; empty for
    /// a fleet without the read grant, and then left off the wire, so a
    /// fleet with no grant reads the reply every runner already parses.
    #[serde(borrow, default, skip_serializing_if = "Vec::is_empty")]
    pub shared: Vec<SharedMemory<'a>>,
    /// Whether this fleet may store an entry the workspace reads; left off
    /// the wire when it may not.
    #[serde(default, skip_serializing_if = "is_false")]
    pub publish: bool,
}

/// Whether a grant is withheld, so the reply leaves it off the wire.
#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde hands skip_serializing_if a reference"
)]
const fn is_false(granted: &bool) -> bool {
    !*granted
}

/// `POST /v1/runners/me/memory/{fleet_id}/recall` request.
//
// Fenced like a push: a holder a reclaim superseded reads nothing.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct MemoryRecallRequest<'a> {
    /// The lease authorizing this read.
    #[serde(borrow)]
    #[garde(length(bytes, min = 1, max = MAX_KEY_LEN))]
    pub lease_id: Cow<'a, str>,
    /// Monotonic guard; a reclaimed holder is rejected.
    #[garde(skip)]
    pub fencing_token: u64,
    /// Text to find in a key or content, ignoring case; empty matches all.
    #[serde(borrow)]
    #[garde(length(bytes, max = MAX_CONTENT_LEN))]
    pub query: Cow<'a, str>,
    /// The most entries of each kind to answer with.
    #[garde(range(min = 1, max = RECALL_LIMIT_MAX))]
    pub limit: usize,
}

/// `POST /v1/runners/me/memory/{fleet_id}/recall` reply: the fleet's own
/// matches and, for a granted reader, the workspace's shared ones.
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRecallResponse<'a> {
    /// The fleet's own entries, key matches first.
    #[serde(borrow)]
    pub memory: Vec<MemoryDelta<'a>>,
    /// Other fleets' shared entries, key matches first.
    #[serde(borrow, default)]
    pub shared: Vec<SharedMemory<'a>>,
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
    /// Who reads it: `fleet` for its writer alone, `workspace` when shared.
    #[serde(default)]
    pub visibility: Visibility,
    /// The fleet that wrote it. Another fleet's identifier marks a shared
    /// entry this fleet reads and cannot change or forget.
    #[serde(borrow)]
    pub writer_fleet_id: Cow<'a, str>,
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
