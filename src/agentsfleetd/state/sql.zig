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
    \\INSERT INTO billing.tenant_wallet
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
    \\UPDATE billing.tenant_wallet
    \\SET balance_nanos = balance_nanos - $2,
    \\    balance_exhausted_at = NULL,
    \\    updated_at = $3
    \\WHERE tenant_id = $1::uuid
    \\  AND balance_nanos >= $2
    \\RETURNING balance_nanos, updated_at
;

pub const SELECT_TENANT_BILLING_EXISTS =
    \\SELECT 1 FROM billing.tenant_wallet WHERE tenant_id = $1::uuid LIMIT 1
;

/// The tenant's own free-trial boundary. NULL means open-ended. Narrow on
/// purpose: the metering path needs only this column, not the whole ledger row.
pub const SELECT_TENANT_TRIAL_BOUNDARY =
    \\SELECT free_trial_ends_at
    \\FROM billing.tenant_wallet
    \\WHERE tenant_id = $1::uuid
    \\LIMIT 1
;

pub const SELECT_TENANT_BALANCE =
    \\SELECT balance_nanos, grant_source, updated_at, balance_exhausted_at, free_trial_ends_at
    \\FROM billing.tenant_wallet
    \\WHERE tenant_id = $1::uuid
    \\LIMIT 1
;

/// Stamp exhaustion once. The `IS NULL` guard makes the first writer the only
/// writer, so the timestamp records when the balance ran out rather than the
/// last time anything noticed.
pub const MARK_BALANCE_EXHAUSTED =
    \\UPDATE billing.tenant_wallet
    \\SET balance_exhausted_at = $2, updated_at = $2
    \\WHERE tenant_id = $1::uuid
    \\  AND balance_exhausted_at IS NULL
    \\RETURNING balance_exhausted_at
;

/// Clear exhaustion on top-up; mirrors the guard above so a no-op reports none.
pub const CLEAR_BALANCE_EXHAUSTED =
    \\UPDATE billing.tenant_wallet
    \\SET balance_exhausted_at = NULL, updated_at = $2
    \\WHERE tenant_id = $1::uuid
    \\  AND balance_exhausted_at IS NOT NULL
    \\RETURNING tenant_id
;

pub const SELECT_TENANT_FOR_WORKSPACE =
    \\SELECT tenant_id::text
    \\FROM core.workspaces
    \\WHERE id = $1::uuid
    \\LIMIT 1
;

// ── Signup bootstrap ────────────────────────────────────────────────────────

/// Resolve an authenticated subject to its owned tenant and first workspace.
/// Joins through `memberships` on the owner role, so a member of someone else's
/// tenant resolves nothing rather than the wrong workspace.
pub const SELECT_BOOTSTRAP_IDENTITY =
    \\SELECT
    \\    u.id::text,
    \\    t.id::text,
    \\    w.id::text,
    \\    w.name
    \\FROM core.users u
    \\JOIN core.memberships m ON m.user_id = u.id AND m.role = 'owner'
    \\JOIN core.tenants t ON t.id = m.tenant_id
    \\JOIN core.workspaces w ON w.tenant_id = t.id AND w.name IS NOT NULL
    \\WHERE u.oidc_subject = $1
    \\ORDER BY w.created_at ASC
    \\LIMIT 1
;

pub const INSERT_TENANT =
    \\INSERT INTO core.tenants
    \\  (id, name, created_at, updated_at)
    \\VALUES ($1::uuid, $2, $3, $3)
;

pub const INSERT_USER =
    \\INSERT INTO core.users
    \\  (id, tenant_id, oidc_subject, email, display_name, created_at, updated_at)
    \\VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $6)
;

pub const INSERT_MEMBERSHIP =
    \\INSERT INTO core.memberships (id, tenant_id, user_id, role, created_at)
    \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5)
;

/// The partial conflict target matches the partial unique index: workspaces
/// with a NULL name are not subject to the per-tenant name uniqueness.
pub const INSERT_WORKSPACE =
    \\INSERT INTO core.workspaces
    \\  (id, tenant_id, name, created_by, created_at)
    \\VALUES ($1::uuid, $2::uuid, $3, $4, $5)
    \\ON CONFLICT (tenant_id, name) WHERE name IS NOT NULL DO NOTHING
;

// ── CLI credentials (`state/cli_credentials.zig`) ───────────────────────────

/// Mint a credential. The partial unique index on (user_id, machine_name)
/// WHERE revoked_at IS NULL is the guard: if a caller inserts without first
/// revoking this machine's live row, the insert fails here rather than leaving
/// two live credentials an operator cannot tell apart.
pub const INSERT_CLI_CREDENTIAL =
    \\INSERT INTO core.cli_credentials
    \\    (id, user_id, tenant_id, machine_name, credential_hash,
    \\     credential_prefix, deployment, created_from_address, created_at)
    \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6, $7, $8, $9)
;

/// The authentication lookup. Filters the digest alone — the unique constraint
/// on `credential_hash` is the whole access path — and returns revocation state
/// for the caller to judge, so a revoked credential is refused with its own
/// code rather than being indistinguishable from an unknown one.
///
/// `u.tenant_id`, not `c.tenant_id`: the joined user row is the authoritative
/// tenant, so the principal carries the same value the authz layer would
/// otherwise re-fetch per request — the mint-time snapshot on the credential
/// row is provenance, never authority.
pub const SELECT_CLI_CREDENTIAL_BY_HASH =
    \\SELECT c.id::text, c.user_id::text, u.tenant_id::text, c.deployment,
    \\       c.revoked_at, u.oidc_subject
    \\FROM core.cli_credentials c
    \\JOIN core.users u ON u.id = c.user_id
    \\WHERE c.credential_hash = $1
    \\LIMIT 1
;

/// Revoke this machine's live credential ahead of minting its replacement.
/// Scoped to one (user, machine): another machine's credential is untouched,
/// which is what lets a second laptop keep working across a re-login.
pub const REVOKE_CLI_CREDENTIAL_FOR_MACHINE =
    \\UPDATE core.cli_credentials
    \\SET revoked_at = $3
    \\WHERE user_id = $1::uuid AND machine_name = $2 AND revoked_at IS NULL
;

/// Revoke one credential by id, scoped to its owner so a caller cannot revoke
/// a credential belonging to somebody else by guessing an identifier.
pub const REVOKE_CLI_CREDENTIAL_BY_ID =
    \\UPDATE core.cli_credentials
    \\SET revoked_at = $3
    \\WHERE id = $1::uuid AND user_id = $2::uuid AND revoked_at IS NULL
;

/// Resolve an authenticated subject to the user row these endpoints write
/// against. `core.cli_credentials.user_id` is a foreign key to `core.users(id)`,
/// but a principal carries the identity provider's subject, so the two are one
/// lookup apart.
///
/// Deliberately narrower than `SELECT_BOOTSTRAP_IDENTITY`, which joins
/// memberships on the owner role and requires a named workspace: a read-only
/// collaborator satisfies neither and would resolve nothing. A collaborator
/// minting a credential for their own terminal is precisely the case the
/// resolved-capability model exists to keep working.
pub const SELECT_USER_IDENTITY_BY_SUBJECT =
    \\SELECT id::text, tenant_id::text
    \\FROM core.users
    \\WHERE oidc_subject = $1
    \\LIMIT 1
;

/// A user's live credentials, newest first. `credential_prefix` is the only
/// credential-shaped column returned, and it does not authenticate.
pub const SELECT_LIVE_CLI_CREDENTIALS_FOR_USER =
    \\SELECT id::text, machine_name, credential_prefix, deployment,
    \\       created_from_address, created_at
    \\FROM core.cli_credentials
    \\WHERE user_id = $1::uuid AND revoked_at IS NULL
    \\ORDER BY created_at DESC
;
