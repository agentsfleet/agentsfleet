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
    /// A person's terminal, authed by an `afc_` credential minted by
    /// `agentsfleet login`. Resolves to the user who created it, never to a
    /// tenant-wide principal — which is what lets a user-scoped route refuse a
    /// tenant key while accepting this.
    ///
    /// Unlike every other source, its capabilities are not granted in code:
    /// the credential proves identity only, and the scope claim is resolved
    /// from the identity provider at request time, so a scope edit reaches a
    /// terminal the same way it reaches the dashboard.
    cli_credential,
};

pub const AuthPrincipal = struct {
    mode: AuthMode,
    user_id: ?[]const u8 = null,
    tenant_id: ?[]const u8 = null,
    workspace_scope_id: ?[]const u8 = null,
    /// Set only when `mode == .runner` — the `fleet.runners` row id resolved
    /// from the presented runner token. Freed with the other principal fields.
    runner_id: ?[]const u8 = null,
    /// Set only when `mode == .runner` — the row's reconciled `degraded`
    /// verdict, carried from the same auth lookup that proved the token so the
    /// lease gate needs no second read of the row it just authenticated
    /// against. Null (any non-runner principal, or a constructor that never
    /// looked) reads as degraded: the gate fails closed.
    runner_degraded: ?bool = null,
    /// Explicit capability set parsed from the verified token's `scopes` claim.
    /// A bitset — no allocation, no lifetime. Hierarchy-expanded at
    /// parse time, so a gate is a single `contains`. Absent claim ⇒ empty set ⇒
    /// every capability gate fails closed. The sole authorization axis on the
    /// principal — `role`/`platform_admin` were removed.
    scopes: ScopeSet = ScopeSet.initEmpty(),
};
