//! `memory.memory_entries` — the durable set a fleet carries between runs.
//!
//! Text is byte-identical to `memory/sql.zig`, except [`ASSUME_MEMORY_ROLE`],
//! which is `SET LOCAL` where the Zig is `SET`. That one word is the whole
//! difference between a role Postgres restores for us and a role a `defer` has
//! to remember to restore — see [`crate::memory`].
//!
//! Every statement is fleet-scoped: `fleet_id` leads each predicate, and the
//! reads ordering by `updated_at` are served by
//! `idx_memory_entries_fleet_id_updated_at_id`.

/// Take the role that may write memory, for this transaction only.
///
/// `SET LOCAL`, so Postgres restores the previous role at COMMIT or ROLLBACK —
/// including the rollback that dropping a transaction performs. `helpers.zig`
/// uses plain `SET` and pairs it with a deferred `RESET ROLE`, whose own
/// documentation admits the failure mode: a reset that errors leaves the
/// connection running as `memory_runtime`, and the pool must discard it.
///
/// No parameter, because a role name cannot be one — which is exactly why it is
/// a constant here rather than anything a caller could pass.
pub const ASSUME_MEMORY_ROLE: &str = "SET LOCAL ROLE memory_runtime";

/// Upsert one entry.
///
/// The stable `(key, fleet_id)` pair is the fleet's own overwrite mechanism — a
/// repeated key replaces rather than accumulates, which is the PRIMARY bound on
/// a fleet's memory growth. The cap below is only a backstop.
///
/// `$1` row id, `$2` key, `$3` content, `$4` category, `$5` fleet, `$6` now.
pub const UPSERT_ENTRY: &str = "\
INSERT INTO memory.memory_entries
  (id, key, content, category, fleet_id, created_at, updated_at)
VALUES ($1::uuid, $2, $3, $4, $5::uuid, $6, $6)
ON CONFLICT (key, fleet_id) DO UPDATE
  SET content = EXCLUDED.content,
      category = EXCLUDED.category,
      updated_at = EXCLUDED.updated_at";

/// Evict past the cap, keeping pinned and recent entries.
///
/// `ORDER BY (category = $3) DESC` sorts the protected category first, so
/// `OFFSET $2` drops the coldest non-core rows and reaches a `core` row only
/// when no other remains. `$3` is
/// [`PINNED_CATEGORY`](crate::memory::window::PINNED_CATEGORY) — the same
/// declaration hydration pins on, which is what stops eviction deleting what
/// hydration promises.
///
/// The leading expression is why this cannot be served pre-ordered by an index:
/// it sorts, by design.
///
/// `$1` fleet, `$2` the cap, `$3` the protected category.
pub const EVICT_PAST_CAP: &str = "\
DELETE FROM memory.memory_entries
WHERE fleet_id = $1::uuid
  AND id IN (
    SELECT id FROM memory.memory_entries
    WHERE fleet_id = $1::uuid
    ORDER BY (category = $3) DESC, updated_at DESC, id DESC
    OFFSET $2
  )";

/// Retention sweep for one category — scratch notes older than a cutoff.
///
/// The category is a PARAMETER, never a pattern, so a category added later
/// cannot accidentally become perishable.
///
/// `$1` fleet, `$2` category, `$3` cutoff.
pub const DELETE_AGED_IN_CATEGORY: &str = "\
DELETE FROM memory.memory_entries
WHERE fleet_id = $1::uuid
  AND category = $2
  AND updated_at < $3";

/// A fleet's whole memory set, newest first.
///
/// Unbounded by design — [`window::select`](crate::memory::window::select)
/// bounds the reply, and bounding here would make the budget a property of the
/// statement instead of the caller spending it.
///
/// `$1` fleet.
pub const SELECT_ALL_FOR_FLEET: &str = "\
SELECT key, content, category
FROM memory.memory_entries
WHERE fleet_id = $1::uuid
ORDER BY updated_at DESC, id DESC";

// ── The operator surface's reads ─────────────────────────────────────────
//
// Copied from `http/handlers/memory/sql.zig`, whose own header carries the
// reasoning: every one is fleet-scoped, bounded, and keyset-paged over
// `(created_at, key)` — `created_at` rather than `updated_at` because an upsert
// moves a row's `updated_at` mid-walk, which is exactly the repeat/skip defect
// cursor paging exists to remove. Served by
// `idx_memory_entries_fleet_id_created_at_key`; the trailing `created_at`
// column feeds the continuation cursor and is not part of the wire item.
//
// Each read has a first-page form and an `_AFTER` form seeking strictly past
// the cursor row with a composite row comparison. Six statements rather than
// one built at run time: a `WHERE` assembled from parts is a `WHERE` a reader
// has to reassemble, and the only enforcement of verbatim parity with the Zig
// is REVIEW reading the two side by side.
//
// The parameter ORDER is the one thing all six share, and
// [`crate::memory::browse`] relies on it: fleet, then the filter value where
// there is one, then the boundary pair where there is one, then the limit. That
// is what lets one bind pipeline serve every shape.

/// Free-text search over a fleet's memory.
///
/// `ESCAPE '\'` is load-bearing: the caller's pattern is built by escaping `%`,
/// `_` and `\`, so a person typing a literal wildcard matches that character
/// rather than every row.
///
/// `$1` fleet, `$2` the escaped pattern, `$3` the limit.
pub const SEARCH_ENTRIES: &str = "\
SELECT key, content, category, updated_at, created_at
FROM memory.memory_entries
WHERE fleet_id = $1::uuid
  AND (key ILIKE $2 ESCAPE '\\' OR content ILIKE $2 ESCAPE '\\')
ORDER BY created_at DESC, key DESC
LIMIT $3";

/// [`SEARCH_ENTRIES`], resuming strictly past a boundary row.
///
/// `$1` fleet, `$2` pattern, `$3` boundary instant, `$4` boundary key,
/// `$5` limit.
pub const SEARCH_ENTRIES_AFTER: &str = "\
SELECT key, content, category, updated_at, created_at
FROM memory.memory_entries
WHERE fleet_id = $1::uuid
  AND (key ILIKE $2 ESCAPE '\\' OR content ILIKE $2 ESCAPE '\\')
  AND (created_at, key) < ($3, $4)
ORDER BY created_at DESC, key DESC
LIMIT $5";

/// One category of a fleet's memory, newest first.
///
/// `$1` fleet, `$2` category, `$3` limit.
pub const SELECT_ENTRIES_IN_CATEGORY: &str = "\
SELECT key, content, category, updated_at, created_at
FROM memory.memory_entries
WHERE fleet_id = $1::uuid AND category = $2
ORDER BY created_at DESC, key DESC LIMIT $3";

/// [`SELECT_ENTRIES_IN_CATEGORY`], resuming strictly past a boundary row.
///
/// `$1` fleet, `$2` category, `$3` boundary instant, `$4` boundary key,
/// `$5` limit.
pub const SELECT_ENTRIES_IN_CATEGORY_AFTER: &str = "\
SELECT key, content, category, updated_at, created_at
FROM memory.memory_entries
WHERE fleet_id = $1::uuid AND category = $2
  AND (created_at, key) < ($3, $4)
ORDER BY created_at DESC, key DESC LIMIT $5";

/// A fleet's memory, newest first, bounded.
///
/// The sibling of [`SELECT_ALL_FOR_FLEET`] and the difference is who reads it:
/// hydration takes everything and lets [`window::select`] spend a byte budget,
/// where a person paging a list takes one page at a time. Different ordering
/// too — `created_at` here, `updated_at` there — for the reason the block
/// header gives.
///
/// [`window::select`]: crate::memory::window::select
///
/// `$1` fleet, `$2` limit.
pub const SELECT_RECENT_ENTRIES: &str = "\
SELECT key, content, category, updated_at, created_at
FROM memory.memory_entries
WHERE fleet_id = $1::uuid
ORDER BY created_at DESC, key DESC LIMIT $2";

/// [`SELECT_RECENT_ENTRIES`], resuming strictly past a boundary row.
///
/// `$1` fleet, `$2` boundary instant, `$3` boundary key, `$4` limit.
pub const SELECT_RECENT_ENTRIES_AFTER: &str = "\
SELECT key, content, category, updated_at, created_at
FROM memory.memory_entries
WHERE fleet_id = $1::uuid
  AND (created_at, key) < ($2, $3)
ORDER BY created_at DESC, key DESC LIMIT $4";

/// Forget one key.
///
/// `RETURNING key` is what separates a real deletion from a no-op, so the
/// caller can answer 404 for a key the fleet was never holding rather than a
/// 204 that would let an operator believe a wrong lesson was removed.
///
/// Keyed on `(fleet_id, key)` and not on `key` alone: two fleets may each hold
/// the same key, and forgetting is one fleet's business.
///
/// `$1` fleet, `$2` key.
pub const DELETE_ENTRY_BY_KEY: &str = "\
DELETE FROM memory.memory_entries
WHERE fleet_id = $1::uuid AND key = $2
RETURNING key";

/// Which workspace owns a fleet.
///
/// Run under the API role BEFORE any role switch, which is the whole reason it
/// is here rather than folded into the reads above as a join:
/// `memory_runtime` cannot see `core`, so the ownership question has to be
/// answered while the connection can still ask it. `helpers.zig` spells the
/// same statement inline for the same reason.
///
/// `$1` fleet.
pub const SELECT_FLEET_WORKSPACE: &str = "\
SELECT workspace_id::text FROM core.fleets WHERE id = $1::uuid";

/// The fleet's live fencing sequence, if this runner holds a live lease on it.
///
/// `COALESCE(a.fencing_seq, l.fencing_token)` so a reclaim that bumped the
/// sequence strands the old holder BELOW it — the affinity row is the live
/// authority and the lease's own token is only the fallback for a fleet whose
/// slot row is gone.
///
/// `$1` runner, `$2` fleet, `$3` the active status, `$4` now.
pub const SELECT_LIVE_FENCE_BY_FLEET: &str = "\
SELECT COALESCE(a.fencing_seq, l.fencing_token) AS live_seq
FROM fleet.runner_leases l
LEFT JOIN fleet.runner_affinity a ON a.fleet_id = l.fleet_id
WHERE l.runner_id = $1::uuid AND l.fleet_id = $2::uuid
  AND l.status = $3 AND l.lease_expires_at > $4
ORDER BY l.created_at DESC
LIMIT 1";

/// The same fence, addressed by lease id when the caller already holds one.
///
/// Keyed by lease AND fleet, so a lease that exists but belongs to another
/// fleet yields no row — the IDOR cross-check IS the `WHERE`, not a comparison
/// the handler has to remember to make afterwards.
///
/// `$1` lease, `$2` runner, `$3` fleet, `$4` the active status, `$5` now.
pub const SELECT_LIVE_FENCE_BY_LEASE: &str = "\
SELECT COALESCE(a.fencing_seq, l.fencing_token) AS live_seq
FROM fleet.runner_leases l
LEFT JOIN fleet.runner_affinity a ON a.fleet_id = l.fleet_id
WHERE l.id = $1::uuid AND l.runner_id = $2::uuid AND l.fleet_id = $3::uuid
  AND l.status = $4 AND l.lease_expires_at > $5
LIMIT 1";
