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

/// Mint. `active` starts true with a null `revoked_at`, which the
/// `api_keys_revoked_iff_inactive` pairing enforces.
pub const INSERT_TENANT_KEY: &str = "\
INSERT INTO core.api_keys \
(id, tenant_id, key_name, description, key_hash, created_by, active, created_at, updated_at) \
VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, TRUE, $7, $7)";

/// Revoke, reporting whether this call was the one that changed the row.
pub const REVOKE_TENANT_KEY: &str = "\
WITH current_row AS ( \
    SELECT id, active \
    FROM core.api_keys \
    WHERE id = $1::uuid AND tenant_id = $2::uuid \
), updated AS ( \
    UPDATE core.api_keys k \
    SET active = FALSE, revoked_at = $3, updated_at = $3 \
    FROM current_row c \
    WHERE k.id = c.id AND c.active = TRUE \
    RETURNING k.id::text, k.revoked_at \
) \
SELECT u.id, u.revoked_at, TRUE AS changed, FALSE AS active \
FROM updated u \
UNION ALL \
SELECT c.id::text, NULL::bigint AS revoked_at, FALSE AS changed, c.active \
FROM current_row c \
WHERE NOT EXISTS (SELECT 1 FROM updated) \
LIMIT 1";

/// Delete, with the same idempotence shape as revoke.
///
/// Deliberately refuses to delete an ACTIVE key (`c.active = FALSE`):
/// revocation is the reversible step and must come first, so a live credential
/// cannot vanish in one call with nothing left to audit.
pub const DELETE_TENANT_KEY: &str = "\
WITH current_row AS ( \
    SELECT id, active \
    FROM core.api_keys \
    WHERE id = $1::uuid AND tenant_id = $2::uuid \
), deleted AS ( \
    DELETE FROM core.api_keys k \
    USING current_row c \
    WHERE k.id = c.id AND c.active = FALSE \
    RETURNING k.id::text \
) \
SELECT d.id, TRUE AS changed, FALSE AS active \
FROM deleted d \
UNION ALL \
SELECT c.id::text, FALSE AS changed, c.active \
FROM current_row c \
WHERE NOT EXISTS (SELECT 1 FROM deleted) \
LIMIT 1";

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
// `afd_api::paging::SortOrder`, whose only constructor is a parse against a
// closed allowlist — there is no expression anywhere that puts a caller's bytes
// in either slot. No index serves these orderings, deliberately: a tenant holds
// roughly a hundred human-created keys, which the page limit already covers.

/// The first page. `{order}` the ORDER BY clause · `$1` tenant, `$2` limit.
pub const SELECT_TENANT_KEY_PAGE_FIRST: &str = "\
WITH tenant_total AS ( \
    SELECT COUNT(*)::bigint AS total FROM core.api_keys WHERE tenant_id = $1::uuid \
) \
SELECT p.id, p.key_name, p.active, p.created_at, p.last_used_at, p.revoked_at, t.total \
FROM tenant_total t \
LEFT JOIN LATERAL ( \
    SELECT id::text AS id, key_name, active, created_at, last_used_at, revoked_at \
    FROM core.api_keys \
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
    SELECT COUNT(*)::bigint AS total FROM core.api_keys WHERE tenant_id = $1::uuid \
) \
SELECT p.id, p.key_name, p.active, p.created_at, p.last_used_at, p.revoked_at, t.total \
FROM tenant_total t \
LEFT JOIN LATERAL ( \
    SELECT id::text AS id, key_name, active, created_at, last_used_at, revoked_at \
    FROM core.api_keys \
    WHERE tenant_id = $1::uuid \
      AND (created_at, id) {comparator} ($2::bigint, $3::uuid) \
    ORDER BY {order} \
    LIMIT $4 \
) p ON TRUE \
ORDER BY {order}";

/// A continuation from a name boundary. Same slots and binds, different column.
pub const SELECT_TENANT_KEY_PAGE_AFTER_NAME: &str = "\
WITH tenant_total AS ( \
    SELECT COUNT(*)::bigint AS total FROM core.api_keys WHERE tenant_id = $1::uuid \
) \
SELECT p.id, p.key_name, p.active, p.created_at, p.last_used_at, p.revoked_at, t.total \
FROM tenant_total t \
LEFT JOIN LATERAL ( \
    SELECT id::text AS id, key_name, active, created_at, last_used_at, revoked_at \
    FROM core.api_keys \
    WHERE tenant_id = $1::uuid \
      AND (key_name, id) {comparator} ($2::text, $3::uuid) \
    ORDER BY {order} \
    LIMIT $4 \
) p ON TRUE \
ORDER BY {order}";

/// The named slot the ORDER BY clause is written into.
pub const SLOT_ORDER: &str = "{order}";

/// The named slot the row-value comparator is written into.
pub const SLOT_COMPARATOR: &str = "{comparator}";
