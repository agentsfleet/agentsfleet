//! Every statement the operator projections run: `runner.leases` joined to the
//! `fleet.events` and `fleet.fleets` rows it names.
//!
//! Each is a `.sql` file beside this module, schema-qualified (RULE NSQ), and
//! checked against the rebuilt schema by `afd_db`'s
//! `test_every_sql_file_prepares`. Every one filters the same way: `workspace`
//! when set, and `fleet` as an identifier or a case-insensitive name when set.

/// The runner's filtered lease total, or no row when the runner does not exist,
/// so an unknown runner and an empty history answer differently.
///
/// `$1` runner · `$2` workspace or NULL · `$3` fleet id or name or NULL.
pub(crate) const SELECT_RUNNER_LEASE_TOTAL: &str = include_str!("select_runner_lease_total.sql");

/// The `created_at` of the cursor lease, when it sits inside the filtered
/// history; no row otherwise.
///
/// `$1` cursor lease · `$2` runner · `$3` workspace or NULL · `$4` fleet id or
/// name or NULL.
pub(crate) const SELECT_RUNNER_LEASE_CURSOR: &str = include_str!("select_runner_lease_cursor.sql");

/// One page of the runner's filtered history, newest first.
///
/// `$1` runner · `$2` workspace or NULL · `$3` fleet id or name or NULL · `$4`
/// page size · `$5` and `$6` the boundary's `created_at` and id, both NULL for
/// the first page, where the boundary falls back to `infinity` so the one
/// statement seeks `idx_leases_runner_id_created_at_id` either way. A lease is
/// a reclaim when an earlier fencing token leased the same event.
pub(crate) const SELECT_RUNNER_LEASE_PAGE: &str = include_str!("select_runner_lease_page.sql");
