//! The tenant api-key statements — the `agt_t` credentials.
//!
//! No statement here ever selects `key_hash` back out. A key's plaintext exists
//! for the length of one response and the digest is written once and compared,
//! never read into anything: a list that returned the digest would be a list
//! that leaked an offline-guessable value to every dashboard tab.
//!
//! # Why revoke and delete are CTEs and not `UPDATE … RETURNING`
//!
//! Both have to answer whether THIS call was the one that changed the row, and
//! a read-then-write would race two operators clicking Revoke. The `UPDATE`
//! fires only on a row in the state it may leave, and the `UNION ALL` answers
//! the pre-existing state when it did not fire — so a second revoke reports
//! "already revoked" rather than a spurious success or a 404 for a row that is
//! plainly there.

/// Mint. A key is live while `revoked_at` is NULL, so a fresh one is live with
/// no column saying so. Answers the `created_at` it stamped.
pub const INSERT_TENANT_KEY: &str = include_str!("apikey/insert_tenant_key.sql");

/// Revoke, reporting whether this call was the one that changed the row, and
/// the `revoked_at` the row holds either way.
pub const REVOKE_TENANT_KEY: &str = include_str!("apikey/revoke_tenant_key.sql");

/// Delete, with the same idempotence shape as revoke.
///
/// Deliberately refuses to delete a LIVE key (`c.revoked_at IS NOT NULL`):
/// revocation is the reversible step and must come first, so a live credential
/// cannot vanish in one call with nothing left to audit.
pub const DELETE_TENANT_KEY: &str = include_str!("apikey/delete_tenant_key.sql");

// ── The list page ───────────────────────────────────────────────────────────
//
// ONE statement carries the page AND the page-stable total. The count subquery
// has no keyset predicate, so `total` is the tenant's whole key count on every
// page; the `LEFT JOIN LATERAL` guarantees at least one row, so an empty page
// still answers a single marker row with a real total and the handler never
// needs a second round trip. The outer `ORDER BY` re-asserts the lateral's
// order so the plan cannot reorder what it produced.
//
// The `{order}` and `{comparator}` slots are filled from
// `afd_core::paging::SortOrder`, whose only constructor is a parse against a
// closed allowlist — there is no expression anywhere that puts a caller's bytes
// in either slot. No index serves these orderings, deliberately: a tenant holds
// roughly a hundred human-created keys, which the page limit already covers.
//
// The three stay Rust constants rather than `.sql` files: the slots are filled
// at run time, so the text as written would not `PREPARE`. Each filled form is
// run by `integration_api_key_paging`.

/// The first page. `{order}` the ORDER BY clause · `$1` tenant, `$2` limit.
pub const SELECT_TENANT_KEY_PAGE_FIRST: &str = "\
WITH tenant_total AS ( \
    SELECT COUNT(*)::bigint AS total FROM identity.api_keys WHERE tenant_id = $1::uuid \
) \
SELECT p.id, p.name, p.created_at, p.revoked_at, t.total \
FROM tenant_total t \
LEFT JOIN LATERAL ( \
    SELECT id::text AS id, name, created_at, revoked_at \
    FROM identity.api_keys \
    WHERE tenant_id = $1::uuid \
    ORDER BY {order} \
    LIMIT $2 \
) p ON TRUE \
ORDER BY {order}";

/// A continuation from a creation-time boundary.
///
/// `{comparator}` the row-value operator · `{order}` the ORDER BY clause ·
/// `$1` tenant, `$2` boundary instant, `$3` boundary id, `$4` limit.
pub const SELECT_TENANT_KEY_PAGE_AFTER_CREATED: &str = "\
WITH tenant_total AS ( \
    SELECT COUNT(*)::bigint AS total FROM identity.api_keys WHERE tenant_id = $1::uuid \
) \
SELECT p.id, p.name, p.created_at, p.revoked_at, t.total \
FROM tenant_total t \
LEFT JOIN LATERAL ( \
    SELECT id::text AS id, name, created_at, revoked_at \
    FROM identity.api_keys \
    WHERE tenant_id = $1::uuid \
      AND (created_at, id) {comparator} ($2::timestamptz, $3::uuid) \
    ORDER BY {order} \
    LIMIT $4 \
) p ON TRUE \
ORDER BY {order}";

/// A continuation from a name boundary. Same slots and binds, different column.
pub const SELECT_TENANT_KEY_PAGE_AFTER_NAME: &str = "\
WITH tenant_total AS ( \
    SELECT COUNT(*)::bigint AS total FROM identity.api_keys WHERE tenant_id = $1::uuid \
) \
SELECT p.id, p.name, p.created_at, p.revoked_at, t.total \
FROM tenant_total t \
LEFT JOIN LATERAL ( \
    SELECT id::text AS id, name, created_at, revoked_at \
    FROM identity.api_keys \
    WHERE tenant_id = $1::uuid \
      AND (name, id) {comparator} ($2::text, $3::uuid) \
    ORDER BY {order} \
    LIMIT $4 \
) p ON TRUE \
ORDER BY {order}";

/// The named slot the ORDER BY clause is written into.
pub const SLOT_ORDER: &str = "{order}";

/// The named slot the row-value comparator is written into.
pub const SLOT_COMPARATOR: &str = "{comparator}";
