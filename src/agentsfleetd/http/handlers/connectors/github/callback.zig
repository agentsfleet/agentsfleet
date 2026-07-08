//! GitHub callback hook — the provider delta the generic callback handler
//! (`connectors/callback.zig`) dispatches to for the `app_install` archetype.
//! GitHub redirects the operator's browser here after the App install; the
//! generic handler has already verified + consumed the signed state and
//! resolved the bound workspace. This hook validates the `installation_id`
//! and writes the `github` vault handle the broker mints from. No token
//! is ever handled — only the installation id.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const logging = @import("log");
const common = @import("../../common.zig");
const hx_mod = @import("../../hx.zig");
const ec = @import("../../../../errors/error_registry.zig");
const vault = @import("../../../../state/vault.zig");
const spec = @import("spec.zig");

const log = logging.scoped(.connector_github);

const Q_INSTALLATION_ID = "installation_id";
// The exact vault-handle shape the broker reads (integration_github.zig).
const HANDLE_FMT = "{{\"integration\":\"github\",\"installation_id\":\"{s}\"}}";
const MAX_INSTALLATION_ID_LEN: usize = 32;

/// Registry `complete` hook for the app_install archetype. Runs AFTER the
/// state is consumed; returns true on success (the generic handler then
/// redirects) and false after having written the failure response itself.
pub fn complete(hx: hx_mod.Hx, workspace_id: []const u8, req: *httpz.Request) bool {
    const qs = req.query() catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "Bad query string");
        return false;
    };
    const installation_id = qs.get(Q_INSTALLATION_ID) orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "Missing installation_id");
        return false;
    };
    if (!isNumericId(installation_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "Malformed installation_id");
        return false;
    }

    storeHandle(hx, workspace_id, installation_id) catch {
        common.internalOperationError(hx.res, "Failed to store GitHub connection", hx.req_id);
        return false;
    };

    log.info("github_connected", .{ .workspace_id = workspace_id });
    return true;
}

fn storeHandle(hx: hx_mod.Hx, workspace_id: []const u8, installation_id: []const u8) !void {
    const conn: *pg.Conn = hx.ctx.pool.acquire() catch return error.DbUnavailable;
    defer hx.ctx.pool.release(conn);

    const handle = try std.fmt.allocPrint(hx.alloc, HANDLE_FMT, .{installation_id});
    defer hx.alloc.free(handle);

    try vault.storeJsonPlaintext(hx.alloc, conn, workspace_id, spec.PROVIDER, handle);
}

fn isNumericId(s: []const u8) bool {
    if (s.len == 0 or s.len > MAX_INSTALLATION_ID_LEN) return false;
    for (s) |c| if (c < '0' or c > '9') return false;
    return true;
}

// ── Tests (pure validation; the vault round-trip is integration-gated) ───────

const testing = std.testing;

test "isNumericId: digits only, bounded length" {
    try testing.expect(isNumericId("12345678"));
    try testing.expect(!isNumericId(""));
    try testing.expect(!isNumericId("12a45"));
    try testing.expect(!isNumericId("-1"));
    try testing.expect(!isNumericId("1" ** 33));
}
