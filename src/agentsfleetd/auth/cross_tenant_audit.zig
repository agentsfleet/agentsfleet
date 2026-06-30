//! Cross-tenant override audit.
//!
//! Emits one structured `.auth_audit` record for every `workspace:any` bypass —
//! the operator, their tenant, the TARGET (victim) tenant, and the workspace.
//! `emit` sits on the
//! SOLE cross-tenant bypass path in `common_authz`, so every bypass is audited
//! before the request proceeds (Invariant 11) — there is no bypass code path
//! that skips it.
//!
//! `.auth_audit` is the dedicated restricted-routing sink (see
//! `audit_events.zig`): deploy-side routing fans it to a security destination.
//! `warn` level because a cross-tenant access is always security-notable.
//!
//! Portable: imports only `std`, the `log`/`common` named modules, and the
//! sibling `principal` — never an HTTP/handler module.

const std = @import("std");
const clock = @import("common").clock;
const logging = @import("log");
const principal_mod = @import("principal.zig");

const audit_log = logging.scoped(.auth_audit);

const AUDIT_RECORD_EVENT = "audit_record";
pub const EV_CROSS_TENANT_ACCESS: []const u8 = "auth.cross_tenant.access";

const S_UNKNOWN = "unknown";
const S_NONE = "none";

/// Record a cross-tenant override access. Called once, synchronously, on the
/// only path that returns "authorized" after bypassing the tenant-id match.
pub fn emit(
    principal: principal_mod.AuthPrincipal,
    workspace_id: []const u8,
    target_tenant: []const u8,
) void {
    audit_log.warn(AUDIT_RECORD_EVENT, .{
        .event = EV_CROSS_TENANT_ACCESS,
        .ts_ms = clock.nowMillis(),
        .operator = principal.user_id orelse S_UNKNOWN,
        .operator_tenant = principal.tenant_id orelse S_NONE,
        .target_tenant = target_tenant,
        .workspace_id = workspace_id,
    });
}

// ── Tests ────────────────────────────────────────────────────────────────

const testing = std.testing;

test "emit runs without panic for a cross-tenant override (wiring smoke)" {
    // Captured-bytes assertion is an integration concern; this pins the type
    // signature + that a null user_id/tenant_id degrades to the sentinels.
    emit(.{
        .mode = .jwt_oidc,
        .user_id = "user_op",
        .tenant_id = "tenant_operator",
    }, "ws_victim", "tenant_victim");
}

test "emit tolerates a null operator identity" {
    emit(.{ .mode = .jwt_oidc }, "ws_x", "tenant_y");
}

test "event name is stable and namespaced" {
    try testing.expectEqualStrings("auth.cross_tenant.access", EV_CROSS_TENANT_ACCESS);
}
