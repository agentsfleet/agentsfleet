//! The statements that open a personal account (RULE SQLMOD — query text lives
//! here, grepable in one place).
//!
//! The workspace insert here is signup's OWN, and the difference from
//! [`super::workspace::INSERT_WORKSPACE`] is load-bearing rather than
//! cosmetic: `workspace::directory` retries a name collision by re-running a
//! standalone insert, while signup's retry happens INSIDE the account
//! transaction, where a raised unique violation would abort every statement
//! that came before it. `ON CONFLICT DO NOTHING` makes a collision a
//! zero-row result the loop can read instead of a poisoned transaction.

/// Resolve an authenticated subject to its owned tenant and first workspace.
///
/// Joins through `memberships` on the owner role, so a member of somebody
/// else's tenant resolves nothing rather than the wrong workspace. `$2` is the
/// role rather than a literal, so it cannot drift from the role the membership
/// insert writes.
pub const SELECT_EXISTING: &str = "\
SELECT u.id::text, t.id::text, w.id::text, w.name \
FROM core.users u \
JOIN core.memberships m ON m.user_id = u.id AND m.role = $2 \
JOIN core.tenants t ON t.id = m.tenant_id \
JOIN core.workspaces w ON w.tenant_id = t.id AND w.name IS NOT NULL \
WHERE u.oidc_subject = $1 \
ORDER BY w.created_at ASC \
LIMIT 1";

/// Open the tenant a personal account hangs from.
pub const INSERT_TENANT: &str = "\
INSERT INTO core.tenants (id, name, created_at, updated_at) \
VALUES ($1::uuid, $2, $3, $3)";

/// Open the person. `oidc_subject` carries the unique index that makes the
/// whole bootstrap idempotent under concurrency.
pub const INSERT_USER: &str = "\
INSERT INTO core.users \
  (id, tenant_id, oidc_subject, email, display_name, created_at, updated_at) \
VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $6)";

/// Link the person to the tenant as its owner.
pub const INSERT_MEMBERSHIP: &str = "\
INSERT INTO core.memberships (id, tenant_id, user_id, role, created_at) \
VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5)";

/// Open the tenant's wallet with its one-time starter balance.
///
/// `DO NOTHING` is what makes this safe to re-run: a replay heals a wallet that
/// went missing without resetting a balance somebody has already spent down.
pub const INSERT_WALLET: &str = "\
INSERT INTO billing.tenant_wallet \
  (tenant_id, balance_nanos, grant_source, created_at, updated_at) \
VALUES ($1::uuid, $2, $3, $4, $4) \
ON CONFLICT (tenant_id) DO NOTHING";

/// Open the default workspace, yielding to a name another draw already took.
///
/// The partial conflict target matches the partial unique index: a workspace
/// with a NULL name is not subject to per-tenant name uniqueness. See the
/// module note on why this is not `workspace::INSERT_WORKSPACE`.
pub const INSERT_WORKSPACE_IF_FREE: &str = "\
INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
VALUES ($1::uuid, $2::uuid, $3, $4, $5) \
ON CONFLICT (tenant_id, name) WHERE name IS NOT NULL DO NOTHING";
