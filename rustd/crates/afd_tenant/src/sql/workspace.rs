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

/// Does this principal's effective tenant own this workspace?
///
/// `$1` workspace id · `$2` the identity provider's subject, or NULL · `$3` the
/// tenant claim, or NULL. Answers the owning tenant when allowed, and NO ROW
/// otherwise — which is what keeps "denied" distinguishable from "the datastore
/// would not answer" all the way up (RULE ECL).
pub const AUTHORIZE_WORKSPACE: &str = "\
SELECT w.tenant_id::text \
FROM core.workspaces w \
WHERE w.id = $1::uuid \
  AND w.tenant_id = COALESCE( \
        (SELECT u.tenant_id FROM core.users u WHERE u.oidc_subject = $2), \
        $3::uuid)";

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

// The four page selects below split what `tenant_workspaces.zig` merges into
// one CTE statement. The merge existed to fold the tenant resolve into the
// page read; here the resolve is `tenant_of`, the ONE statement every tenant
// route shares — a second spelling of the authority order to save its round
// trip would be two places for that order to drift apart. The walk itself is
// the Zig one: oldest first, `(created_at, id)` keyset, exact-name filter.

/// The first page of a tenant's workspaces.
pub const SELECT_TENANT_WORKSPACES_PAGE_FIRST: &str = "\
SELECT id::text, name, created_at \
FROM core.workspaces \
WHERE tenant_id = $1::uuid \
ORDER BY created_at ASC, id ASC \
LIMIT $2";

/// The page after a boundary row.
pub const SELECT_TENANT_WORKSPACES_PAGE_AFTER: &str = "\
SELECT id::text, name, created_at \
FROM core.workspaces \
WHERE tenant_id = $1::uuid \
  AND (created_at, id) > ($2, $3::uuid) \
ORDER BY created_at ASC, id ASC \
LIMIT $4";

/// The first page, held to an exact name.
///
/// The filter a client reconciling its own create uses, so it can find the
/// row it just made without walking the whole list.
pub const SELECT_TENANT_WORKSPACES_PAGE_FIRST_BY_NAME: &str = "\
SELECT id::text, name, created_at \
FROM core.workspaces \
WHERE tenant_id = $1::uuid \
  AND name = $2 \
ORDER BY created_at ASC, id ASC \
LIMIT $3";

/// The page after a boundary row, held to an exact name.
pub const SELECT_TENANT_WORKSPACES_PAGE_AFTER_BY_NAME: &str = "\
SELECT id::text, name, created_at \
FROM core.workspaces \
WHERE tenant_id = $1::uuid \
  AND name = $2 \
  AND (created_at, id) > ($3, $4::uuid) \
ORDER BY created_at ASC, id ASC \
LIMIT $5";
