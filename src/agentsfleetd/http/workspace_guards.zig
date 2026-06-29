const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const common = @import("handlers/common.zig");
const error_codes = @import("../errors/error_registry.zig");

/// Caller-owned allocator: methods that allocate (incl. deinit) take the allocator as a parameter.
pub const Access = struct {
    const Self = @This();

    /// Reserved for future per-request state. Currently empty; `deinit` is a
    /// no-op so existing call sites (`defer access.deinit(alloc)`) stay valid.
    pub fn deinit(self: Self, alloc: std.mem.Allocator) void {
        _ = self;
        _ = alloc;
    }
};

fn authorizeWorkspace(
    res: *httpz.Response,
    req_id: []const u8,
    conn: *pg.Conn,
    principal: common.AuthPrincipal,
    workspace_id: []const u8,
) bool {
    if (common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id)) return true;
    common.errorResponse(res, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
    return false;
}

/// Workspace ownership gate.
///
/// Capability ("may this principal write fleets?") is enforced UPSTREAM by the
/// `requireScope` middleware before the handler runs, so this gate is now purely
/// the resource/ownership axis: it verifies the principal owns the workspace (or
/// holds the audited `workspace:any` cross-tenant override) and writes the RLS
/// tenant context. The former role check + workspace-creator `user→operator`
/// auto-promotion are gone — capability rides the token (provisioned via the
/// `.tenant` default grant at signup), and the two axes stay separate.
pub fn enforce(
    res: *httpz.Response,
    req_id: []const u8,
    conn: *pg.Conn,
    alloc: std.mem.Allocator,
    principal: common.AuthPrincipal,
    workspace_id: []const u8,
    actor: []const u8,
) ?Access {
    _ = alloc;
    _ = actor;
    if (!authorizeWorkspace(res, req_id, conn, principal, workspace_id)) return null;
    return .{};
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

test "Access.deinit is a no-op" {
    const access = Access{};
    access.deinit(std.testing.allocator);
}
