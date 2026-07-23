//! SQL statement text for the fleet-memory domain (RULE SQLMOD — query text
//! lives here, grepable in one place).
//!
//! Every statement is fleet-scoped: `fleet_id` leads each predicate, and the
//! reads that order by `updated_at` are served by
//! `idx_memory_entries_fleet_id_updated_at_id` (schema slot 033).

/// Upsert one entry. The stable `(key, fleet_id)` pair is the fleet's own
/// overwrite mechanism — a repeated key replaces rather than accumulates, which
/// is the primary bound on a fleet's memory growth.
pub const UPSERT_ENTRY =
    \\INSERT INTO memory.memory_entries
    \\  (uid, id, key, content, category, fleet_id, created_at, updated_at)
    \\VALUES ($1::uuid, $2, $3, $4, $5, $6::uuid, $7, $7)
    \\ON CONFLICT (key, fleet_id) DO UPDATE
    \\  SET content = EXCLUDED.content,
    \\      category = EXCLUDED.category,
    \\      updated_at = EXCLUDED.updated_at
;

/// Evict past the cap, newest-and-core first. The ordering keeps `core` entries
/// and recent ones; `OFFSET $2` is the cap, so everything past it is dropped.
/// The leading `(category = $3)` expression is why this read cannot be served
/// pre-ordered by an index — it sorts, by design.
pub const EVICT_PAST_CAP =
    \\DELETE FROM memory.memory_entries
    \\WHERE fleet_id = $1::uuid
    \\  AND id IN (
    \\    SELECT id FROM memory.memory_entries
    \\    WHERE fleet_id = $1::uuid
    \\    ORDER BY (category = $3) DESC, updated_at DESC, id DESC
    \\    OFFSET $2
    \\  )
;

/// Retention sweep for one category — scratch notes older than a cutoff.
pub const DELETE_AGED_IN_CATEGORY =
    \\DELETE FROM memory.memory_entries
    \\WHERE fleet_id = $1::uuid
    \\  AND category = $2
    \\  AND updated_at < $3
;

/// Forget one key. `RETURNING key` distinguishes a real deletion from a no-op
/// so the caller can report whether anything was forgotten.
pub const DELETE_ENTRY_BY_KEY =
    \\DELETE FROM memory.memory_entries
    \\WHERE fleet_id = $1::uuid AND key = $2
    \\RETURNING key
;

/// Hydration: a fleet's whole memory set, newest first. Unbounded by design —
/// the fleet receives all of it — which is why the planner sorts here rather
/// than reading the composite index in order.
pub const SELECT_ALL_FOR_FLEET =
    \\SELECT key, content, category
    \\FROM memory.memory_entries
    \\WHERE fleet_id = $1::uuid
    \\ORDER BY updated_at DESC, id DESC
;
