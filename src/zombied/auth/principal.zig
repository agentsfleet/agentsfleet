//! Authenticated principal populated by auth middleware.
//!
//! Owned by `src/auth/` so the folder can be extracted into a standalone
//! `zombie-auth` repository without reaching into HTTP/business layers.
//! `src/http/handlers/common.zig` re-exports these symbols for backward
//! compatibility during the M18_002 migration.

const rbac = @import("rbac.zig");

pub const AuthRole = rbac.AuthRole;

pub const AuthMode = enum {
    api_key,
    jwt_oidc,
    /// Host-resident `zombie-runner`, authed by a `zrn_` runner token via
    /// `runnerBearer`. Carries no tenant identity (`tenant_id == null`).
    runner,
};

pub const AuthPrincipal = struct {
    mode: AuthMode,
    role: AuthRole = .user,
    user_id: ?[]const u8 = null,
    tenant_id: ?[]const u8 = null,
    workspace_scope_id: ?[]const u8 = null,
    /// Set only when `mode == .runner` — the `fleet.runners` row id resolved
    /// from the presented runner token. Freed with the other principal fields.
    runner_id: ?[]const u8 = null,
};
