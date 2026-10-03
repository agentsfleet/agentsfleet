//! `memory.memory_entries` — every statement the Postgres store runs.
//!
//! The runner-plane writes are byte-identical to `memory/sql.zig` apart from
//! the two columns slot 926 added, and [`ASSUME_MEMORY_ROLE`], which is
//! `SET LOCAL` where the Zig is `SET`: Postgres restores the role at COMMIT or
//! ROLLBACK, so there is no reset to fail and no connection can return to the
//! pool running as `memory_runtime`.
//!
//! Every read returns the same seven columns in the same order — writer, key,
//! content, category, visibility, created, updated — so one decoder reads them
//! all. A fleet's own rows lead each predicate with `fleet_id`; a shared read
//! leads with `workspace_id` and is served by the partial shared index.

/// Take the role that may write memory, for this transaction only.
pub(super) const ASSUME_MEMORY_ROLE: &str = "SET LOCAL ROLE memory_runtime";

/// Upsert one entry.
///
/// The stable `(key, fleet_id)` pair is the fleet's own overwrite mechanism —
/// a repeated key replaces rather than accumulates, which is the PRIMARY bound
/// on a fleet's memory growth. The cap below is only a backstop.
///
/// `$1` row id, `$2` key, `$3` content, `$4` category, `$5` fleet,
/// `$6` workspace, `$7` workspace-visible, `$8` now.
pub(super) const UPSERT_ENTRY: &str = "\
INSERT INTO memory.memory_entries
  (id, key, content, category, fleet_id, workspace_id, workspace_visible, created_at, updated_at)
VALUES ($1::uuid, $2, $3, $4, $5::uuid, $6::uuid, $7, $8, $8)
ON CONFLICT (key, fleet_id) DO UPDATE
  SET content = EXCLUDED.content,
      category = EXCLUDED.category,
      workspace_visible = EXCLUDED.workspace_visible,
      updated_at = EXCLUDED.updated_at";

/// Copy one entry in as it stands, keeping any newer row already here.
///
/// The flip's rule: a push that landed in this store during the copy carries a
/// later `updated_at` than the snapshot row, and the `WHERE` keeps it.
///
/// `$1` row id, `$2` key, `$3` content, `$4` category, `$5` writer fleet,
/// `$6` workspace, `$7` workspace-visible, `$8` created, `$9` updated.
pub(super) const IMPORT_ENTRY: &str = "\
INSERT INTO memory.memory_entries
  (id, key, content, category, fleet_id, workspace_id, workspace_visible, created_at, updated_at)
VALUES ($1::uuid, $2, $3, $4, $5::uuid, $6::uuid, $7, $8, $9)
ON CONFLICT (key, fleet_id) DO UPDATE
  SET content = EXCLUDED.content,
      category = EXCLUDED.category,
      workspace_visible = EXCLUDED.workspace_visible,
      created_at = EXCLUDED.created_at,
      updated_at = EXCLUDED.updated_at
  WHERE memory.memory_entries.updated_at < EXCLUDED.updated_at";

/// Evict past the cap, keeping pinned and recent entries.
///
/// `ORDER BY (category = $3) DESC` sorts the protected category first, so
/// `OFFSET $2` drops the coldest non-core rows and reaches a `core` row only
/// when no other remains. `$3` is the category hydration pins on, which is
/// what stops eviction deleting what hydration promises.
///
/// `$1` fleet, `$2` the cap, `$3` the protected category.
pub(super) const EVICT_PAST_CAP: &str = "\
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
/// `$1` fleet, `$2` category, `$3` cutoff.
pub(super) const DELETE_AGED_IN_CATEGORY: &str = "\
DELETE FROM memory.memory_entries
WHERE fleet_id = $1::uuid
  AND category = $2
  AND updated_at < $3";

/// A fleet's whole memory set, newest first — hydration's input, unbounded
/// because the window spends the budget, not the statement.
///
/// `$1` fleet.
pub(super) const SELECT_ALL_FOR_FLEET: &str = "\
SELECT fleet_id::text, key, content, category, workspace_visible, created_at, updated_at
FROM memory.memory_entries
WHERE fleet_id = $1::uuid
ORDER BY updated_at DESC, id DESC";

/// Other fleets' shared entries in a workspace, newest first.
///
/// `$1` workspace, `$2` the reading fleet, `$3` the most rows to read.
pub(super) const SELECT_SHARED_IN_WORKSPACE: &str = "\
SELECT fleet_id::text, key, content, category, workspace_visible, created_at, updated_at
FROM memory.memory_entries
WHERE workspace_id = $1::uuid AND workspace_visible AND fleet_id <> $2::uuid
ORDER BY updated_at DESC, id DESC
LIMIT $3";

/// A fleet's own entries matching a pattern, key matches first.
///
/// `ESCAPE '\'` is load-bearing: the caller escapes `%`, `_` and `\`, so a
/// literal wildcard matches that character rather than every row.
///
/// `$1` fleet, `$2` the escaped pattern, `$3` limit.
pub(super) const SEARCH_OWN: &str = "\
SELECT fleet_id::text, key, content, category, workspace_visible, created_at, updated_at
FROM memory.memory_entries
WHERE fleet_id = $1::uuid
  AND (key ILIKE $2 ESCAPE '\\' OR content ILIKE $2 ESCAPE '\\')
ORDER BY (key ILIKE $2 ESCAPE '\\') DESC, updated_at DESC, id DESC
LIMIT $3";

/// Other fleets' shared entries matching a pattern, key matches first.
///
/// `$1` workspace, `$2` the reading fleet, `$3` the escaped pattern, `$4` limit.
pub(super) const SEARCH_SHARED: &str = "\
SELECT fleet_id::text, key, content, category, workspace_visible, created_at, updated_at
FROM memory.memory_entries
WHERE workspace_id = $1::uuid AND workspace_visible AND fleet_id <> $2::uuid
  AND (key ILIKE $3 ESCAPE '\\' OR content ILIKE $3 ESCAPE '\\')
ORDER BY (key ILIKE $3 ESCAPE '\\') DESC, updated_at DESC, id DESC
LIMIT $4";

/// Every entry in a workspace, for a flip's copy.
///
/// `$1` workspace.
pub(super) const SELECT_WORKSPACE_ENTRIES: &str = "\
SELECT fleet_id::text, key, content, category, workspace_visible, created_at, updated_at
FROM memory.memory_entries
WHERE workspace_id = $1::uuid
ORDER BY fleet_id, key";

/// Forget one key — the fleet's own row and never another's.
///
/// `RETURNING key` separates a real deletion from a no-op, so the caller can
/// answer 404 for a key the fleet was never holding.
///
/// `$1` fleet, `$2` key.
pub(super) const DELETE_ENTRY_BY_KEY: &str = "\
DELETE FROM memory.memory_entries
WHERE fleet_id = $1::uuid AND key = $2
RETURNING key";
