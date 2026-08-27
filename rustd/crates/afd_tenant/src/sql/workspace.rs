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
