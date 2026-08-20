//! SQL statement text for the workspace-authorization funnel (RULE SQLMOD —
//! query text lives in a domain sql module, grepable in one place; the sibling
//! `common_authz.zig` owns verdicts, buffers, and error translation).
//!
//! The two AUTHORIZE statements are the same decision; the `_SET_CONTEXT`
//! variant additionally writes the Row-Level Security (RLS) session context in
//! the same round trip. `set_config` sits in the SELECT list, so it evaluates
//! only when the WHERE clause matched — a denied request cannot leak a tenant
//! onto the pooled connection because the row that would carry the write never
//! exists. That placement is the load-bearing property; a separate statement
//! after the verdict is how the pre-merge shape spent an extra round trip.
//!
//! Tenant authority order, encoded in the COALESCE: the `core.users` row named
//! by the OpenID Connect (OIDC) subject outranks the token's tenant claim; the
//! claim only decides when no user row exists (claim-bound API keys and
//! Command-Line Interface (CLI) credentials bind `$2` as NULL, so their claim
//! is the whole authority — their auth lookup already read the user row).

/// One-round-trip verdict: does this principal's effective tenant own the
/// workspace? `$1` workspace id · `$2` OIDC subject or NULL · `$3` tenant
/// claim or NULL. Returns the owning tenant id when allowed; no row otherwise.
pub const AUTHORIZE_WORKSPACE =
    \\SELECT w.tenant_id::text
    \\FROM core.workspaces w
    \\WHERE w.id = $1::uuid
    \\  AND w.tenant_id = COALESCE(
    \\        (SELECT u.tenant_id FROM core.users u WHERE u.oidc_subject = $2),
    \\        $3::uuid)
;

/// The verdict plus the RLS context write, one round trip. Same binds as
/// `AUTHORIZE_WORKSPACE`; the second column's value is unused (the write is
/// the point), but selecting it keeps the call sites' row shape explicit.
pub const AUTHORIZE_WORKSPACE_SET_CONTEXT =
    \\SELECT w.tenant_id::text,
    \\       set_config('app.current_tenant_id', w.tenant_id::text, false)
    \\FROM core.workspaces w
    \\WHERE w.id = $1::uuid
    \\  AND w.tenant_id = COALESCE(
    \\        (SELECT u.tenant_id FROM core.users u WHERE u.oidc_subject = $2),
    \\        $3::uuid)
;

/// Standalone user→tenant resolve for the cold paths that need the tenant
/// WITHOUT a workspace to check it against (workspace create, tenant lists).
pub const SELECT_USER_TENANT_BY_SUBJECT =
    \\SELECT tenant_id::text FROM core.users WHERE oidc_subject = $1 LIMIT 1
;

/// Standalone RLS context write for callers that resolved the tenant through
/// another statement (workspace create seeds context before its insert).
pub const SET_TENANT_CONTEXT =
    \\SELECT set_config('app.current_tenant_id', $1, false)
;

/// The audited cross-tenant bypass resolves the TARGET workspace's tenant so
/// the operator acts inside the victim tenant's row scope.
pub const SELECT_WORKSPACE_TENANT =
    \\SELECT tenant_id::text FROM core.workspaces WHERE id = $1::uuid
;

/// Fleet→workspace resolve for routes addressed by fleet id alone.
pub const SELECT_FLEET_WORKSPACE =
    \\SELECT workspace_id::text FROM core.fleets WHERE id = $1::uuid LIMIT 1
;
