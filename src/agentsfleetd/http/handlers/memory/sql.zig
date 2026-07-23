//! SQL statement text for the tenant-facing memory reads (RULE SQLMOD — query
//! text lives here, grepable in one place).
//!
//! Read-only by design: every durable write goes through
//! `memory/fleet_memory.zig`, which owns the single INSERT path. All three
//! reads are fleet-scoped and bounded, and each is served pre-ordered by
//! `idx_memory_entries_fleet_id_updated_at_id` (schema slot 033).

/// Free-text search over a fleet's memory.
///
/// `ESCAPE '\'` is load-bearing: the caller's pattern is built by escaping `%`,
/// `_` and `\` (see `state/fleet_events_filter.zig`), so a user typing a
/// literal wildcard matches that character rather than every row.
pub const SEARCH_ENTRIES =
    \\SELECT key, content, category, updated_at
    \\FROM memory.memory_entries
    \\WHERE fleet_id = $1::uuid
    \\  AND (key ILIKE $2 ESCAPE '\' OR content ILIKE $2 ESCAPE '\')
    \\ORDER BY updated_at DESC, id DESC
    \\LIMIT $3
;

pub const SELECT_ENTRIES_IN_CATEGORY =
    \\SELECT key, content, category, updated_at
    \\FROM memory.memory_entries
    \\WHERE fleet_id = $1::uuid AND category = $2
    \\ORDER BY updated_at DESC, id DESC LIMIT $3
;

pub const SELECT_RECENT_ENTRIES =
    \\SELECT key, content, category, updated_at
    \\FROM memory.memory_entries
    \\WHERE fleet_id = $1::uuid
    \\ORDER BY updated_at DESC, id DESC LIMIT $2
;
