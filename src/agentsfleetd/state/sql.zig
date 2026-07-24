//! SQL statement text for the store layer (RULE SQLMOD — query text lives here,
//! grepable in one place).
//!
//! `state/` is the durable-read layer behind the handlers: billing balances,
//! signup bootstrap, and the small lookups that resolve one identifier to
//! another. Sub-domains with their own directory (`user_preferences`,
//! `tenant_provider`, `model_library`, …) keep their own `sql.zig`.

// ── Tenant billing ──────────────────────────────────────────────────────────

/// Open a tenant's billing row. `DO NOTHING` makes bootstrap idempotent — a
/// re-run never resets a balance that already exists.
pub const INSERT_TENANT_BILLING =
    \\INSERT INTO billing.tenant_billing
    \\  (tenant_id, balance_nanos, grant_source, created_at, updated_at)
    \\VALUES ($1::uuid, $2, $3, $4, $4)
    \\ON CONFLICT (tenant_id) DO NOTHING
;

/// Debit, refusing to go negative.
///
/// `balance_nanos >= $2` in the WHERE is the overdraft guard, and it is why
/// this is one statement rather than a read-then-write: two concurrent debits
/// cannot both observe a sufficient balance and both succeed. A caller that
/// gets no row was outbid, not errored.
pub const DEBIT_TENANT_BALANCE =
    \\UPDATE billing.tenant_billing
    \\SET balance_nanos = balance_nanos - $2,
    \\    balance_exhausted_at = NULL,
    \\    updated_at = $3
    \\WHERE tenant_id = $1::uuid
    \\  AND balance_nanos >= $2
    \\RETURNING balance_nanos, updated_at
;

pub const SELECT_TENANT_BILLING_EXISTS =
    \\SELECT 1 FROM billing.tenant_billing WHERE tenant_id = $1::uuid LIMIT 1
;

pub const SELECT_TENANT_BALANCE =
    \\SELECT balance_nanos, grant_source, updated_at, balance_exhausted_at
    \\FROM billing.tenant_billing
    \\WHERE tenant_id = $1::uuid
    \\LIMIT 1
;

/// Stamp exhaustion once. The `IS NULL` guard makes the first writer the only
/// writer, so the timestamp records when the balance ran out rather than the
/// last time anything noticed.
pub const MARK_BALANCE_EXHAUSTED =
    \\UPDATE billing.tenant_billing
    \\SET balance_exhausted_at = $2, updated_at = $2
    \\WHERE tenant_id = $1::uuid
    \\  AND balance_exhausted_at IS NULL
    \\RETURNING balance_exhausted_at
;

/// Clear exhaustion on top-up; mirrors the guard above so a no-op reports none.
pub const CLEAR_BALANCE_EXHAUSTED =
    \\UPDATE billing.tenant_billing
    \\SET balance_exhausted_at = NULL, updated_at = $2
    \\WHERE tenant_id = $1::uuid
    \\  AND balance_exhausted_at IS NOT NULL
    \\RETURNING tenant_id
;

pub const SELECT_TENANT_FOR_WORKSPACE =
    \\SELECT tenant_id::text
    \\FROM core.workspaces
    \\WHERE workspace_id = $1::uuid
    \\LIMIT 1
;

// ── Signup bootstrap ────────────────────────────────────────────────────────

/// Resolve an authenticated subject to its owned tenant and first workspace.
/// Joins through `memberships` on the owner role, so a member of someone else's
/// tenant resolves nothing rather than the wrong workspace.
pub const SELECT_BOOTSTRAP_IDENTITY =
    \\SELECT
    \\    u.user_id::text,
    \\    t.tenant_id::text,
    \\    w.workspace_id::text,
    \\    w.name
    \\FROM core.users u
    \\JOIN core.memberships m ON m.user_id = u.user_id AND m.role = 'owner'
    \\JOIN core.tenants t ON t.tenant_id = m.tenant_id
    \\JOIN core.workspaces w ON w.tenant_id = t.tenant_id AND w.name IS NOT NULL
    \\WHERE u.oidc_subject = $1
    \\ORDER BY w.created_at ASC
    \\LIMIT 1
;

pub const INSERT_TENANT =
    \\INSERT INTO core.tenants
    \\  (tenant_id, name, created_at, updated_at)
    \\VALUES ($1::uuid, $2, $3, $3)
;

pub const INSERT_USER =
    \\INSERT INTO core.users
    \\  (user_id, tenant_id, oidc_subject, email, display_name, created_at, updated_at)
    \\VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $6)
;

pub const INSERT_MEMBERSHIP =
    \\INSERT INTO core.memberships (uid, tenant_id, user_id, role, created_at)
    \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5)
;

/// The partial conflict target matches the partial unique index: workspaces
/// with a NULL name are not subject to the per-tenant name uniqueness.
pub const INSERT_WORKSPACE =
    \\INSERT INTO core.workspaces
    \\  (workspace_id, tenant_id, name, created_by, created_at)
    \\VALUES ($1::uuid, $2::uuid, $3, $4, $5)
    \\ON CONFLICT (tenant_id, name) WHERE name IS NOT NULL DO NOTHING
;
