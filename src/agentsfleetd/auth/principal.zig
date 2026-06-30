//! Authenticated principal populated by auth middleware.
//!
//! Owned by `src/auth/` so the folder can be extracted into a standalone
//! `fleet-auth` repository without reaching into HTTP/business layers.
//! `src/http/handlers/common.zig` re-exports these symbols for backward
//! compatibility during the M18_002 migration.

const scopes = @import("scopes.zig");

pub const Scope = scopes.Scope;
pub const ScopeSet = scopes.Set;

pub const AuthMode = enum {
    api_key,
    jwt_oidc,
    /// Host-resident `agentsfleet-runner`, authed by a `agt_r` runner token via
    /// `runnerBearer`. Carries no tenant identity (`tenant_id == null`).
    runner,
};

pub const AuthPrincipal = struct {
    mode: AuthMode,
    user_id: ?[]const u8 = null,
    tenant_id: ?[]const u8 = null,
    workspace_scope_id: ?[]const u8 = null,
    /// Set only when `mode == .runner` — the `fleet.runners` row id resolved
    /// from the presented runner token. Freed with the other principal fields.
    runner_id: ?[]const u8 = null,
    /// Explicit capability set parsed from the verified token's `scopes` claim.
    /// A bitset — no allocation, no lifetime. Hierarchy-expanded at
    /// parse time, so a gate is a single `contains`. Absent claim ⇒ empty set ⇒
    /// every capability gate fails closed. The sole authorization axis on the
    /// principal — `role`/`platform_admin` were removed.
    scopes: ScopeSet = ScopeSet.initEmpty(),
};
