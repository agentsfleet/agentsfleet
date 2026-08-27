//! The billing reads — the wallet snapshot and the charges ledger.
//!
//! Nothing here writes. The wallet's one inflow is the signup starter grant,
//! and every drain is a writable CTE on the lease plane; this module is the
//! read side those movements are audited through.
//!
//! # The select list is spelled twice, deliberately
//!
//! Both page forms open with the same thirteen columns. `concat!` takes only
//! literals, so sharing the prefix would mean a `String` built at runtime —
//! and sqlx takes a `String` only through `AssertSqlSafe`, an audit obligation
//! these constants do not otherwise need. Two spellings of a select list in ONE
//! file, adjacent, is a smaller risk than a safety waiver; the sibling
//! [`super::apikey`] makes the same trade for the same reason.
//!
//! The identity columns are UUID, not TEXT, and the `::text` casts are
//! load-bearing: a driver reading a UUID column as text without one hands back
//! raw bytes. `workspace_id` and `fleet_id` are nullable — both foreign keys
//! are ON DELETE SET NULL, so a charge outlives the fleet and workspace it was
//! incurred on. `token_count_cached_input` is deliberately NOT selected: the
//! charges response does not carry it.
//!
//! # `usage_ledger.id` is table-qualified in the ORDER BY
//!
//! The qualification is load-bearing: the select list emits `id::text`, which
//! names an OUTPUT column `id`, and a bare `ORDER BY … id` resolves against the
//! output list before the table — sorting by the TEXT cast, an ordering no
//! index supplies. The WHERE clause never had the problem; output aliases are
//! not visible there.

/// The wallet snapshot behind `GET /v1/tenants/me/billing`.
pub const SELECT_TENANT_BALANCE: &str = "\
SELECT balance_nanos, updated_at, balance_exhausted_at \
FROM billing.tenant_wallet \
WHERE tenant_id = $1::uuid \
LIMIT 1";

/// The first charges page: newest first, no boundary. `$1` tenant, `$2` limit.
pub const SELECT_TENANT_CHARGES_PAGE_FIRST: &str = "\
SELECT id::text, tenant_id::text, workspace_id::text, fleet_id::text, event_id, \
charge_type, posture, model, \
credit_deducted_nanos, \
token_count_input, token_count_output, wall_ms, \
created_at \
FROM billing.usage_ledger \
WHERE tenant_id = $1 \
ORDER BY created_at DESC, usage_ledger.id DESC \
LIMIT $2";

/// A later page: strictly older than the cursor's boundary. `$1` tenant,
/// `$2` boundary instant, `$3` boundary id, `$4` limit.
pub const SELECT_TENANT_CHARGES_PAGE_AFTER: &str = "\
SELECT id::text, tenant_id::text, workspace_id::text, fleet_id::text, event_id, \
charge_type, posture, model, \
credit_deducted_nanos, \
token_count_input, token_count_output, wall_ms, \
created_at \
FROM billing.usage_ledger \
WHERE tenant_id = $1 \
AND (created_at, id) < ($2, $3) \
ORDER BY created_at DESC, usage_ledger.id DESC \
LIMIT $4";
