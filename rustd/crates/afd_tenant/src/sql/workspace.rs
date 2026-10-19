//! The statements the workspace-ownership verdict is decided by.
//!
//! # One round trip, and why the shape is not obvious
//!
//! The effective tenant and the workspace match resolve TOGETHER. A reader
//! expects two statements — resolve who the caller is, then check the row — and
//! that spends two to three sequential round trips. Folding them is what makes
//! an ownership check affordable on EVERY workspace request, which is in turn
//! what makes it affordable as a shared layer rather than as something each
//! handler decides whether to pay for.
//!
//! # The authority order lives in the `COALESCE`
//!
//! The `identity.users` row named by the identity provider's subject OUTRANKS the
//! token's tenant claim. The claim decides only when no user row exists — which
//! is every claim-bound credential, because an `agt_t` or `afc_` lookup already
//! read the user row at authentication time and put the answer on the
//! principal. Those bind `$2` as NULL, so their claim is the whole authority.
//!
//! # What is deliberately not here
//!
//! The verdict statement does not carry `set_config('app.current_tenant_id', …)`
//! for Row-Level Security. Nothing would read that setting: this repository
//! declares no `ROW LEVEL SECURITY` policy and no
//! `current_setting('app.current_tenant_id')` anywhere. Adding it would also
//! need a transaction, because `sqlx` returns a connection to the pool between
//! requests and a session-level setting would leak one tenant's identifier onto
//! the next request.

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
pub const AUTHORIZE_WORKSPACE: &str = include_str!("workspace/authorize_workspace.sql");

/// The tenant owning one workspace, for the audited cross-tenant override.
///
/// Read only AFTER the verdict above has denied, and only for a principal
/// holding the platform-wide scope — so it is the second statement of a path
/// almost nobody takes rather than a cost on the ordinary one.
pub const SELECT_WORKSPACE_TENANT: &str = include_str!("workspace/select_workspace_tenant.sql");

/// The tenant a subject belongs to, with no workspace to check it against.
///
/// The cold path: creating a workspace, and the tenant-scoped lists that carry
/// no workspace identifier at all. `resolvePrincipalTenant`'s statement.
pub const SELECT_USER_TENANT_BY_SUBJECT: &str =
    include_str!("workspace/select_user_tenant_by_subject.sql");

/// Does the tenant a session claims actually exist?
///
/// Asked because a stale session can name a deleted tenant, and refusing it
/// here with the session sentence beats letting the insert's foreign key
/// answer as a 500.
pub const SELECT_TENANT_EXISTS: &str = include_str!("workspace/select_tenant_exists.sql");

/// One workspace row.
///
/// The casts are load-bearing: both identity columns are UUID and the driver
/// sends text. No `ON CONFLICT` clause on purpose — the
/// near-twin in the signup path swallows the collision, while this one needs
/// `uq_workspaces_tenant_id_name` to surface so the caller hears "taken".
pub const INSERT_WORKSPACE: &str = include_str!("workspace/insert_workspace.sql");

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
pub const SELECT_SUBJECT_ACCOUNTS: &str = include_str!("workspace/select_subject_accounts.sql");

/// The one account a claim-bound credential holds: its own.
///
/// `$1` the tenant claim · `$2` the owner role's spelling. No row when the
/// claim names no tenant, and the list is then empty rather than refused.
pub const SELECT_TENANT_ACCOUNT: &str = include_str!("workspace/select_tenant_account.sql");

/// The first page of the workspaces across a person's accounts.
pub const SELECT_TENANT_WORKSPACES_PAGE_FIRST: &str =
    include_str!("workspace/select_tenant_workspaces_page_first.sql");

/// The page after a boundary row.
pub const SELECT_TENANT_WORKSPACES_PAGE_AFTER: &str =
    include_str!("workspace/select_tenant_workspaces_page_after.sql");

/// The first page, held to an exact name.
///
/// The filter a client reconciling its own create uses, so it can find the
/// row it just made without walking the whole list.
pub const SELECT_TENANT_WORKSPACES_PAGE_FIRST_BY_NAME: &str =
    include_str!("workspace/select_tenant_workspaces_page_first_by_name.sql");

/// The page after a boundary row, held to an exact name.
pub const SELECT_TENANT_WORKSPACES_PAGE_AFTER_BY_NAME: &str =
    include_str!("workspace/select_tenant_workspaces_page_after_by_name.sql");
