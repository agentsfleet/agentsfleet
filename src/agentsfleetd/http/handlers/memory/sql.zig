//! SQL statement text for the tenant-facing memory reads (RULE SQLMOD — query
//! text lives here, grepable in one place).
//!
//! Read-only by design: every durable write goes through
//! `memory/fleet_memory.zig`, which owns the single INSERT path. All three
//! reads are fleet-scoped, bounded, and keyset-paged over (created_at, key) —
//! created_at rather than updated_at because an upsert moves a row's
//! updated_at mid-walk, which is exactly the repeat/skip defect cursor paging
//! exists to remove. Served by idx_memory_entries_fleet_id_created_at_key
//! (schema slot 039); the trailing created_at column feeds the continuation
//! cursor and is not part of the wire item.
//!
//! Each read has a first-page form and an `_AFTER` form seeking strictly past
//! the cursor row via a composite row comparison.

/// Free-text search over a fleet's memory.
///
/// `ESCAPE '\'` is load-bearing: the caller's pattern is built by escaping `%`,
/// `_` and `\` (see `state/fleet_events_filter.zig`), so a user typing a
/// literal wildcard matches that character rather than every row.
pub const SEARCH_ENTRIES =
    \\SELECT key, content, category, updated_at, created_at
    \\FROM memory.memory_entries
    \\WHERE fleet_id = $1::uuid
    \\  AND (key ILIKE $2 ESCAPE '\' OR content ILIKE $2 ESCAPE '\')
    \\ORDER BY created_at DESC, key DESC
    \\LIMIT $3
;

pub const SEARCH_ENTRIES_AFTER =
    \\SELECT key, content, category, updated_at, created_at
    \\FROM memory.memory_entries
    \\WHERE fleet_id = $1::uuid
    \\  AND (key ILIKE $2 ESCAPE '\' OR content ILIKE $2 ESCAPE '\')
    \\  AND (created_at, key) < ($3, $4)
    \\ORDER BY created_at DESC, key DESC
    \\LIMIT $5
;

pub const SELECT_ENTRIES_IN_CATEGORY =
    \\SELECT key, content, category, updated_at, created_at
    \\FROM memory.memory_entries
    \\WHERE fleet_id = $1::uuid AND category = $2
    \\ORDER BY created_at DESC, key DESC LIMIT $3
;

pub const SELECT_ENTRIES_IN_CATEGORY_AFTER =
    \\SELECT key, content, category, updated_at, created_at
    \\FROM memory.memory_entries
    \\WHERE fleet_id = $1::uuid AND category = $2
    \\  AND (created_at, key) < ($3, $4)
    \\ORDER BY created_at DESC, key DESC LIMIT $5
;

pub const SELECT_RECENT_ENTRIES =
    \\SELECT key, content, category, updated_at, created_at
    \\FROM memory.memory_entries
    \\WHERE fleet_id = $1::uuid
    \\ORDER BY created_at DESC, key DESC LIMIT $2
;

pub const SELECT_RECENT_ENTRIES_AFTER =
    \\SELECT key, content, category, updated_at, created_at
    \\FROM memory.memory_entries
    \\WHERE fleet_id = $1::uuid
    \\  AND (created_at, key) < ($2, $3)
    \\ORDER BY created_at DESC, key DESC LIMIT $4
;
