//! SQL statement text for the api-key handler domain (RULE SQLMOD — query text
//! lives here, grepable in one place).
//!
//! Two key families share this domain: tenant api-keys (`core.api_keys`, the
//! `agt_t` credentials) and per-fleet keys (`core.fleet_keys`). Neither
//! statement family ever selects `key_hash` back out — a key's plaintext exists
//! only at mint time, and the hash is written once and compared, never read
//! into a response.

// ── Tenant api-keys ─────────────────────────────────────────────────────────

/// Mint. `active` starts TRUE with a null `revoked_at`, the pairing
/// `api_keys_revoked_iff_inactive` enforces.
pub const INSERT_TENANT_KEY =
    \\INSERT INTO core.api_keys (uid, tenant_id, key_name, description, key_hash, created_by, active, created_at, updated_at)
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
    \\    SELECT uid, active
    \\    FROM core.api_keys
    \\    WHERE uid = $1::uuid AND tenant_id = $2::uuid
    \\), updated AS (
    \\    UPDATE core.api_keys k
    \\    SET active = FALSE, revoked_at = $3, updated_at = $3
    \\    FROM current_row c
    \\    WHERE k.uid = c.uid AND c.active = TRUE
    \\    RETURNING k.uid::text, k.revoked_at
    \\)
    \\SELECT u.uid, u.revoked_at, TRUE AS changed, FALSE AS active
    \\FROM updated u
    \\UNION ALL
    \\SELECT c.uid::text, NULL::bigint AS revoked_at, FALSE AS changed, c.active
    \\FROM current_row c
    \\WHERE NOT EXISTS (SELECT 1 FROM updated)
    \\LIMIT 1
;

/// Delete, same idempotence shape as revoke. Deliberately refuses to delete an
/// ACTIVE key (`c.active = FALSE`): revocation is the reversible step and must
/// come first, so a live credential cannot vanish in one call.
pub const DELETE_TENANT_KEY =
    \\WITH current_row AS (
    \\    SELECT uid, active
    \\    FROM core.api_keys
    \\    WHERE uid = $1::uuid AND tenant_id = $2::uuid
    \\), deleted AS (
    \\    DELETE FROM core.api_keys k
    \\    USING current_row c
    \\    WHERE k.uid = c.uid AND c.active = FALSE
    \\    RETURNING k.uid::text
    \\)
    \\SELECT d.uid, TRUE AS changed, FALSE AS active
    \\FROM deleted d
    \\UNION ALL
    \\SELECT c.uid::text, FALSE AS changed, c.active
    \\FROM current_row c
    \\WHERE NOT EXISTS (SELECT 1 FROM deleted)
    \\LIMIT 1
;

/// The tenant key list: one page plus a total that survives an offset past the
/// end. Two `{s}` slots, both the ORDER BY clause, both fed from
/// `sortClauseFor`'s fixed allowlist — never from user input. Each supported
/// ordering is served by an index from schema slot 033, so no sort node runs.
/// `$1` tenant_id, `$2` limit, `$3` offset.
pub const SELECT_TENANT_KEY_PAGE_FMT =
    \\WITH total AS (
    \\    SELECT COUNT(*)::bigint AS total
    \\    FROM core.api_keys
    \\    WHERE tenant_id = $1::uuid
    \\),
    \\page AS (
    \\    SELECT uid, key_name, active, created_at, last_used_at, revoked_at
    \\    FROM core.api_keys
    \\    WHERE tenant_id = $1::uuid
    \\    ORDER BY {s}
    \\    LIMIT $2 OFFSET $3
    \\),
    \\page_rows AS (
    \\    SELECT uid::text, key_name, active, created_at, last_used_at, revoked_at,
    \\           (SELECT total FROM total)::bigint AS total, false AS count_only,
    \\           ROW_NUMBER() OVER (ORDER BY {s})::bigint AS page_ord
    \\    FROM page
    \\),
    \\empty_page AS (
    \\    SELECT ''::text, ''::text, false, 0::bigint, NULL::bigint, NULL::bigint,
    \\           total, true, NULL::bigint
    \\    FROM total
    \\    WHERE NOT EXISTS (SELECT 1 FROM page)
    \\)
    \\SELECT * FROM page_rows
    \\UNION ALL
    \\SELECT * FROM empty_page
    \\ORDER BY count_only ASC, page_ord ASC NULLS LAST
;

// ── Per-fleet keys ──────────────────────────────────────────────────────────

/// Existence + ownership check before minting a fleet key: the fleet must live
/// in the caller's workspace, so a valid fleet id from another tenant fails.
pub const SELECT_FLEET_IN_WORKSPACE =
    \\SELECT 1 FROM core.fleets WHERE id = $1::uuid AND workspace_id = $2::uuid LIMIT 1
;

pub const INSERT_FLEET_KEY =
    \\INSERT INTO core.fleet_keys
    \\  (uid, fleet_key_id, workspace_id, fleet_id, name, description, key_hash, created_at)
    \\VALUES ($1::uuid, $1, $2::uuid, $3::uuid, $4, $5, $6, $7)
;

pub const SELECT_FLEET_KEYS_FOR_WORKSPACE =
    \\SELECT fleet_key_id, fleet_id::text, name, description, created_at, last_used_at
    \\FROM core.fleet_keys
    \\WHERE workspace_id = $1::uuid
    \\ORDER BY created_at DESC
;

/// `RETURNING` distinguishes a real deletion from a no-op, so the handler can
/// answer 404 rather than a false success.
pub const DELETE_FLEET_KEY =
    \\DELETE FROM core.fleet_keys
    \\WHERE fleet_key_id = $1 AND workspace_id = $2::uuid
    \\RETURNING fleet_key_id
;
