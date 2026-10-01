//! The statements the workspace-ownership verdict is decided by.
//!
//! # One round trip, and why the shape is not obvious
//!
//! The effective tenant and the workspace match resolve TOGETHER. A reader
//! expects two statements — resolve who the caller is, then check the row — and
//! the pre-merge Zig shape spent two to three sequential round trips on exactly
//! that. Folding them is what makes an ownership check affordable on EVERY
//! workspace request, which is in turn what makes it affordable as a shared
//! layer rather than as something each handler decides whether to pay for.
//!
//! # The authority order lives in the `COALESCE`
//!
//! The `core.users` row named by the identity provider's subject OUTRANKS the
//! token's tenant claim. The claim decides only when no user row exists — which
//! is every claim-bound credential, because an `agt_t` or `afc_` lookup already
//! read the user row at authentication time and put the answer on the
//! principal. Those bind `$2` as NULL, so their claim is the whole authority.
//!
//! # What is deliberately not here
//!
//! `common_authz_sql.zig` has a second copy of the verdict statement carrying
//! `set_config('app.current_tenant_id', …)` in its select list, for Row-Level
//! Security. Nothing reads that setting: this repository declares no
//! `ROW LEVEL SECURITY` policy and no `current_setting('app.current_tenant_id')`
//! anywhere, so it is written at three sites and read at zero. It is left
//! unported as a declared divergence — the milestone's Discovery log carries
//! the evidence. Re-adding it would also need a transaction, because `sqlx`
//! returns a connection to the pool between requests and a session-level
//! setting would leak one tenant's identifier onto the next request.

/// May this principal open this workspace, and with which role?
///
/// `$1` workspace id · `$2` the identity provider's subject, or NULL · `$3` the
/// tenant claim, or NULL. Answers the owning tenant and the caller's stored
/// role when allowed, and NO ROW otherwise — which is what keeps "denied"
/// distinguishable from "the datastore would not answer" all the way up
/// (RULE ECL).
///
/// Two arms admit. A membership row for the subject's user in the owning
/// tenant admits with that row's role. The caller's own account — the user
/// row's tenant, or the claim when no user row exists — admits whether or not
/// a membership row backs it, and a claim-bound credential (`$2` NULL)
/// reaches nothing else. The role column is
/// NULL on that second arm when no membership row backs it.
///
/// One statement and no subquery: the workspace by primary key, the user by
/// `uq_users_oidc_subject`, the membership by `uq_memberships_tenant_id_user_id`.
/// Each probe returns at most one row, so the plan is three index lookups
/// whatever the table sizes.
pub const AUTHORIZE_WORKSPACE: &str = "\
SELECT w.tenant_id::text, m.role \
FROM core.workspaces w \
LEFT JOIN core.users u ON u.oidc_subject = $2 \
LEFT JOIN core.memberships m ON m.tenant_id = w.tenant_id AND m.user_id = u.id \
WHERE w.id = $1::uuid \
  AND (m.id IS NOT NULL OR w.tenant_id = COALESCE(u.tenant_id, $3::uuid))";

/// The tenant owning one workspace, for the audited cross-tenant override.
///
/// Read only AFTER the verdict above has denied, and only for a principal
/// holding the platform-wide scope — so it is the second statement of a path
/// almost nobody takes rather than a cost on the ordinary one.
pub const SELECT_WORKSPACE_TENANT: &str = "\
SELECT tenant_id::text \
FROM core.workspaces \
WHERE id = $1::uuid";

/// The tenant a subject belongs to, with no workspace to check it against.
///
/// The cold path: creating a workspace, and the tenant-scoped lists that carry
/// no workspace identifier at all. `resolvePrincipalTenant`'s statement.
pub const SELECT_USER_TENANT_BY_SUBJECT: &str = "\
SELECT tenant_id::text \
FROM core.users \
WHERE oidc_subject = $1 \
LIMIT 1";

/// Does the tenant a session claims actually exist?
///
/// `sql.zig`'s `TENANT_EXISTS`, and asked for `lifecycle.zig`'s reason: a
/// stale session can name a deleted tenant, and refusing it here with the
/// session sentence beats letting the insert's foreign key answer as a 500.
pub const SELECT_TENANT_EXISTS: &str = "\
SELECT 1 FROM core.tenants WHERE id = $1::uuid LIMIT 1";

/// One workspace row.
///
/// `sql.zig`'s `INSERT_WORKSPACE`, casts and all: both identity columns are
/// UUID and the driver sends text. No `ON CONFLICT` clause on purpose — the
/// near-twin in the signup path swallows the collision, while this one needs
/// `uq_workspaces_tenant_id_name` to surface so the caller hears "taken".
pub const INSERT_WORKSPACE: &str = "\
INSERT INTO core.workspaces (id, tenant_id, name, created_by, created_at) \
VALUES ($1::uuid, $2::uuid, $3, $4, $5)";

// The list is two statements: the accounts a caller holds, then one keyset
// page across all of them. Folding the account read into the page would put a
// second spelling of the access arms beside `AUTHORIZE_WORKSPACE`, and two
// spellings drift. The walk is unchanged: oldest first, `(created_at, id)`
// keyset, exact-name filter, now over `tenant_id = ANY(...)`, which
// `idx_workspaces_tenant_id_created_at_id` serves per account.

/// The accounts a signed-in person holds, with their role in each.
///
/// `$1` the identity provider's subject · `$2` the owner role's spelling.
/// Every membership row, plus the person's own account whether or not a
/// membership row backs it: the same two arms [`AUTHORIZE_WORKSPACE`] admits
/// by. The last column is the person's own account, repeated on every row.
/// No row at all means no user row, and the caller falls back to the claim.
pub const SELECT_SUBJECT_ACCOUNTS: &str = concat!(
    "WITH me AS ( \
       SELECT id, tenant_id FROM core.users WHERE oidc_subject = $1 \
     ), held AS ( \
       SELECT DISTINCT ON (arm.tenant_id) arm.tenant_id, arm.role FROM ( \
         SELECT m.tenant_id, m.role FROM core.memberships m JOIN me ON m.user_id = me.id \
         UNION ALL \
         SELECT me.tenant_id, NULL::text FROM me \
       ) arm ORDER BY arm.tenant_id, arm.role NULLS LAST \
     ) \
     SELECT t.id::text AS tenant_id, held.role, ",
    owner_name_column!(),
    ", me.tenant_id::text AS home_tenant_id \
     FROM held CROSS JOIN me \
     JOIN core.tenants t ON t.id = held.tenant_id ",
    owner_name_join!(2)
);

/// The one account a claim-bound credential holds: its own.
///
/// `$1` the tenant claim · `$2` the owner role's spelling. No row when the
/// claim names no tenant, and the list is then empty rather than refused.
pub const SELECT_TENANT_ACCOUNT: &str = concat!(
    "SELECT t.id::text AS tenant_id, NULL::text AS role, ",
    owner_name_column!(),
    ", t.id::text AS home_tenant_id \
     FROM core.tenants t ",
    owner_name_join!(2),
    "WHERE t.id = $1::uuid"
);

/// The first page of the workspaces across a person's accounts.
pub const SELECT_TENANT_WORKSPACES_PAGE_FIRST: &str = "\
SELECT id::text, name, created_at, tenant_id::text \
FROM core.workspaces \
WHERE tenant_id = ANY($1::uuid[]) \
ORDER BY created_at ASC, id ASC \
LIMIT $2";

/// The page after a boundary row.
pub const SELECT_TENANT_WORKSPACES_PAGE_AFTER: &str = "\
SELECT id::text, name, created_at, tenant_id::text \
FROM core.workspaces \
WHERE tenant_id = ANY($1::uuid[]) \
  AND (created_at, id) > ($2, $3::uuid) \
ORDER BY created_at ASC, id ASC \
LIMIT $4";

/// The first page, held to an exact name.
///
/// The filter a client reconciling its own create uses, so it can find the
/// row it just made without walking the whole list.
pub const SELECT_TENANT_WORKSPACES_PAGE_FIRST_BY_NAME: &str = "\
SELECT id::text, name, created_at, tenant_id::text \
FROM core.workspaces \
WHERE tenant_id = ANY($1::uuid[]) \
  AND name = $2 \
ORDER BY created_at ASC, id ASC \
LIMIT $3";

/// The page after a boundary row, held to an exact name.
pub const SELECT_TENANT_WORKSPACES_PAGE_AFTER_BY_NAME: &str = "\
SELECT id::text, name, created_at, tenant_id::text \
FROM core.workspaces \
WHERE tenant_id = ANY($1::uuid[]) \
  AND name = $2 \
  AND (created_at, id) > ($3, $4::uuid) \
ORDER BY created_at ASC, id ASC \
LIMIT $5";

#[cfg(test)]
mod tests {
    use super::{SELECT_SUBJECT_ACCOUNTS, SELECT_TENANT_ACCOUNT};

    /// The macro is handed a parameter NUMBER; the statements must come out
    /// comparing the owner role to that parameter, or every account would be
    /// named by nobody.
    #[test]
    fn the_owner_name_join_compares_the_role_to_the_slot_it_is_given() {
        for statement in [SELECT_SUBJECT_ACCOUNTS, SELECT_TENANT_ACCOUNT] {
            assert!(statement.contains("om.role = $2 AND"), "{statement}");
        }
    }
}
