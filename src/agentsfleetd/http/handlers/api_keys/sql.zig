//! SQL statement text for the api-key handler domain (RULE SQLMOD — query text
//! lives here, grepable in one place).
//!
//! Tenant api-keys (`core.api_keys`, the `agt_t` credentials). No statement
//! here ever selects `key_hash` back out — a key's plaintext exists only at
//! mint time, and the hash is written once and compared, never read into a
//! response.
//!
//! The per-fleet `core.fleet_keys` family retired with its surface (M154 §8).

// ── Tenant api-keys ─────────────────────────────────────────────────────────

/// Mint. `active` starts TRUE with a null `revoked_at`, the pairing
/// `api_keys_revoked_iff_inactive` enforces.
pub const INSERT_TENANT_KEY =
    \\INSERT INTO core.api_keys (id, tenant_id, key_name, description, key_hash, created_by, active, created_at, updated_at)
    \\VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, TRUE, $7, $7)
;

/// Revoke, reporting whether THIS call was the one that changed the row.
///
/// The CTE makes revocation idempotent without a read-then-write race: the
/// UPDATE only fires on a currently-active row, and the UNION returns the
/// pre-existing state when it did not. A caller revoking twice gets
/// `changed = FALSE` rather than a spurious success or a 404.
pub const REVOKE_TENANT_KEY =
    \\WITH current_row AS (
    \\    SELECT id, active
    \\    FROM core.api_keys
    \\    WHERE id = $1::uuid AND tenant_id = $2::uuid
    \\), updated AS (
    \\    UPDATE core.api_keys k
    \\    SET active = FALSE, revoked_at = $3, updated_at = $3
    \\    FROM current_row c
    \\    WHERE k.id = c.id AND c.active = TRUE
    \\    RETURNING k.id::text, k.revoked_at
    \\)
    \\SELECT u.id, u.revoked_at, TRUE AS changed, FALSE AS active
    \\FROM updated u
    \\UNION ALL
    \\SELECT c.id::text, NULL::bigint AS revoked_at, FALSE AS changed, c.active
    \\FROM current_row c
    \\WHERE NOT EXISTS (SELECT 1 FROM updated)
    \\LIMIT 1
;

/// Delete, same idempotence shape as revoke. Deliberately refuses to delete an
/// ACTIVE key (`c.active = FALSE`): revocation is the reversible step and must
/// come first, so a live credential cannot vanish in one call.
pub const DELETE_TENANT_KEY =
    \\WITH current_row AS (
    \\    SELECT id, active
    \\    FROM core.api_keys
    \\    WHERE id = $1::uuid AND tenant_id = $2::uuid
    \\), deleted AS (
    \\    DELETE FROM core.api_keys k
    \\    USING current_row c
    \\    WHERE k.id = c.id AND c.active = FALSE
    \\    RETURNING k.id::text
    \\)
    \\SELECT d.id, TRUE AS changed, FALSE AS active
    \\FROM deleted d
    \\UNION ALL
    \\SELECT c.id::text, FALSE AS changed, c.active
    \\FROM current_row c
    \\WHERE NOT EXISTS (SELECT 1 FROM deleted)
    \\LIMIT 1
;

// ── List page — ONE statement carries the page AND the page-stable total ────
// The count CTE has no keyset predicate, so `total` is the tenant's whole key
// count on every page (what the separate count read used to answer). The LEFT
// JOIN LATERAL guarantees at least one row: an empty page still returns a
// single marker row (NULL key columns, real total), so the handler never needs
// a second round trip to learn the total. The outer ORDER BY names output
// aliases, re-asserting the lateral's order so the plan cannot reorder rows.
//
// Positional `{0s}`/`{1s}` slots come from `sortSpecFor`'s fixed allowlist,
// never from user input. No index serves these orderings, deliberately: a
// tenant holds roughly a hundred human-created keys, which the page limit
// already covers. Sorting at that size is free.

/// First page. `{0s}` ORDER BY clause · `$1` tenant_id, `$2` limit.
pub const SELECT_TENANT_KEY_KEYSET_FIRST_FMT =
    \\WITH tenant_total AS (
    \\    SELECT COUNT(*)::bigint AS total FROM core.api_keys WHERE tenant_id = $1::uuid
    \\)
    \\SELECT p.id, p.key_name, p.active, p.created_at, p.last_used_at, p.revoked_at, t.total
    \\FROM tenant_total t
    \\LEFT JOIN LATERAL (
    \\    SELECT id::text AS id, key_name, active, created_at, last_used_at, revoked_at
    \\    FROM core.api_keys
    \\    WHERE tenant_id = $1::uuid
    \\    ORDER BY {0s}
    \\    LIMIT $2
    \\) p ON TRUE
    \\ORDER BY {0s}
;

/// Continuation for the created_at orderings. `{0s}` row-value comparator,
/// `{1s}` ORDER BY clause · `$1` tenant_id, `$2` boundary created_at,
/// `$3` boundary id, `$4` limit.
pub const SELECT_TENANT_KEY_KEYSET_AFTER_CREATED_FMT =
    \\WITH tenant_total AS (
    \\    SELECT COUNT(*)::bigint AS total FROM core.api_keys WHERE tenant_id = $1::uuid
    \\)
    \\SELECT p.id, p.key_name, p.active, p.created_at, p.last_used_at, p.revoked_at, t.total
    \\FROM tenant_total t
    \\LEFT JOIN LATERAL (
    \\    SELECT id::text AS id, key_name, active, created_at, last_used_at, revoked_at
    \\    FROM core.api_keys
    \\    WHERE tenant_id = $1::uuid
    \\      AND (created_at, id) {0s} ($2::bigint, $3::uuid)
    \\    ORDER BY {1s}
    \\    LIMIT $4
    \\) p ON TRUE
    \\ORDER BY {1s}
;

/// Continuation for the key_name orderings — the boundary sort value is the
/// text key the cursor carried. Same allowlist-fed positional slots.
/// `$1` tenant_id, `$2` boundary key_name, `$3` boundary id, `$4` limit.
pub const SELECT_TENANT_KEY_KEYSET_AFTER_NAME_FMT =
    \\WITH tenant_total AS (
    \\    SELECT COUNT(*)::bigint AS total FROM core.api_keys WHERE tenant_id = $1::uuid
    \\)
    \\SELECT p.id, p.key_name, p.active, p.created_at, p.last_used_at, p.revoked_at, t.total
    \\FROM tenant_total t
    \\LEFT JOIN LATERAL (
    \\    SELECT id::text AS id, key_name, active, created_at, last_used_at, revoked_at
    \\    FROM core.api_keys
    \\    WHERE tenant_id = $1::uuid
    \\      AND (key_name, id) {0s} ($2::text, $3::uuid)
    \\    ORDER BY {1s}
    \\    LIMIT $4
    \\) p ON TRUE
    \\ORDER BY {1s}
;

// ── Per-fleet keys ──────────────────────────────────────────────────────────
