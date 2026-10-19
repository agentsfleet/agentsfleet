//! `memory.entries` — every statement the Postgres store runs, each a `.sql`
//! file beside this module and checked against the rebuilt schema by
//! `afd_db`'s `test_every_sql_file_prepares`.
//!
//! [`ASSUME_MEMORY_ROLE`] is `SET LOCAL` rather than `SET`: Postgres restores
//! the role at COMMIT or ROLLBACK, so there is no reset to fail and no
//! connection can return to the pool running as `memory_runtime`.
//!
//! Every read returns the same seven columns in the same order — writer, key,
//! content, category, visibility, created, updated — so one decoder reads them
//! all. A fleet's own rows lead each predicate with `fleet_id`; a shared read
//! leads with `workspace_id` and is served by the partial shared index.

/// Take the role that may write memory, for this transaction only.
pub(crate) const ASSUME_MEMORY_ROLE: &str = include_str!("assume_memory_role.sql");

/// Upsert one entry.
///
/// The stable `(fleet_id, key)` pair is the fleet's own overwrite mechanism —
/// a repeated key replaces rather than accumulates, which is the PRIMARY bound
/// on a fleet's memory growth. The cap below is only a backstop.
///
/// `$1` row id, `$2` key, `$3` content, `$4` category, `$5` fleet,
/// `$6` workspace, `$7` workspace-visible, `$8` now.
pub(crate) const UPSERT_ENTRY: &str = include_str!("upsert_entry.sql");

/// Evict past the cap, keeping pinned and recent entries.
///
/// `ORDER BY (category = $3) DESC` sorts the protected category first, so
/// `OFFSET $2` drops the coldest non-core rows and reaches a `core` row only
/// when no other remains. `$3` is the category hydration pins on, which is
/// what stops eviction deleting what hydration promises.
///
/// `$1` fleet, `$2` the cap, `$3` the protected category.
pub(crate) const EVICT_PAST_CAP: &str = include_str!("evict_past_cap.sql");

/// Retention sweep for one category — scratch notes older than a cutoff.
///
/// `$1` fleet, `$2` category, `$3` cutoff.
pub(crate) const DELETE_AGED_IN_CATEGORY: &str = include_str!("delete_aged_in_category.sql");

/// A fleet's whole memory set, newest first — hydration's input, unbounded
/// because the window spends the budget, not the statement.
///
/// `$1` fleet.
pub(crate) const SELECT_ALL_FOR_FLEET: &str = include_str!("select_all_for_fleet.sql");

/// Other fleets' shared entries in a workspace, newest first.
///
/// `$1` workspace, `$2` the reading fleet, `$3` the most rows to read.
pub(crate) const SELECT_SHARED_IN_WORKSPACE: &str = include_str!("select_shared_in_workspace.sql");

/// A fleet's own entries matching a pattern, key matches first.
///
/// `ESCAPE '\'` is load-bearing: the caller escapes `%`, `_` and `\`, so a
/// literal wildcard matches that character rather than every row.
///
/// `$1` fleet, `$2` the escaped pattern, `$3` limit.
pub(crate) const SEARCH_OWN: &str = include_str!("search_own.sql");

/// Other fleets' shared entries matching a pattern, key matches first.
///
/// `$1` workspace, `$2` the reading fleet, `$3` the escaped pattern, `$4` limit.
pub(crate) const SEARCH_SHARED: &str = include_str!("search_shared.sql");

/// Forget one key — the fleet's own row and never another's.
///
/// `RETURNING key` separates a real deletion from a no-op, so the caller can
/// answer 404 for a key the fleet was never holding.
///
/// `$1` fleet, `$2` key.
pub(crate) const DELETE_ENTRY_BY_KEY: &str = include_str!("delete_entry_by_key.sql");
