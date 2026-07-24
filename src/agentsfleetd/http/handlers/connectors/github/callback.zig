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
const clock = @import("common").clock;
const common = @import("../../common.zig");
const hx_mod = @import("../../hx.zig");
const ec = @import("../../../../errors/error_registry.zig");
const vault = @import("../../../../state/vault.zig");
const id_format = @import("../../../../types/id_format.zig");
const connector_state = @import("../state.zig");
const spec = @import("spec.zig");
const sql = @import("sql.zig");
const ownership = @import("ownership.zig");
const PgQuery = @import("../../../../db/pg_query.zig").PgQuery;

const log = logging.scoped(.connector_github);

const Q_INSTALLATION_ID = "installation_id";
const Q_CODE = "code";
const INSTALLED_BY_UNKNOWN = "";
// The exact vault-handle shape the broker reads (integration_github.zig).
const HANDLE_FMT = "{{\"integration\":\"github\",\"installation_id\":\"{s}\"}}";
const MAX_INSTALLATION_ID_LEN: usize = 32;
const S_STATE_STALE = "Stale GitHub connect state";

/// Registry `complete` hook for the app_install archetype. The generic handler
/// has verified the signed state; this hook consumes the latest-state marker
/// adjacent to final persistence.
pub fn complete(hx: hx_mod.Hx, workspace_id: []const u8, raw_state: []const u8, req: *httpz.Request) bool {
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

    const code = qs.get(Q_CODE) orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "Missing code");
        return false;
    };
    ownership.verify(hx, code, installation_id) catch |err| return failOwnership(hx, err);

    storeHandle(hx, workspace_id, raw_state, installation_id) catch |err| return failOwnership(hx, err);

    log.info("github_connected", .{ .workspace_id = workspace_id });
    return true;
}

fn storeHandle(hx: hx_mod.Hx, workspace_id: []const u8, raw_state: []const u8, installation_id: []const u8) !void {
    const conn: *pg.Conn = hx.ctx.pool.acquire() catch return error.DbUnavailable;
    defer hx.ctx.pool.release(conn);

    const handle = try std.fmt.allocPrint(hx.alloc, HANDLE_FMT, .{installation_id});
    defer hx.alloc.free(handle);

    const uid = try id_format.generateConnectorInstallId(hx.alloc);
    defer hx.alloc.free(uid);
    const no_scopes: []const []const u8 = &.{};
    const now = clock.nowMillis();

    try conn.begin();
    errdefer conn.rollback() catch |err| log.warn("github_connect_rollback_failed", .{
        .error_code = ec.ERR_INTERNAL_OPERATION_FAILED,
        .err = @errorName(err),
    });
    try lockInstallPersistence(conn, workspace_id);
    const is_latest = connector_state.consumeLatest(hx.ctx.queue, spec.STATE, workspace_id, raw_state) catch return error.StateVerifyFailed;
    if (!is_latest) return error.StaleState;
    _ = try conn.exec(sql.DELETE_WORKSPACE_INSTALLS, .{ spec.PROVIDER, workspace_id });
    try vault.storeJsonPlaintext(hx.alloc, conn, workspace_id, spec.PROVIDER, handle);
    {
        var query = PgQuery.from(try conn.query(sql.UPSERT_INSTALL, .{
            uid,
            spec.PROVIDER,
            installation_id,
            workspace_id,
            INSTALLED_BY_UNKNOWN,
            no_scopes,
            now,
        }));
        defer query.deinit();
        if (try query.next() == null) return error.OwnershipDenied;
    }
    try conn.commit();
}

fn lockInstallPersistence(conn: *pg.Conn, workspace_id: []const u8) !void {
    var query = PgQuery.from(try conn.query(sql.LOCK_INSTALL_PERSISTENCE, .{ spec.PROVIDER, workspace_id }));
    defer query.deinit();
    _ = try query.next() orelse return error.LockFailed;
}

fn failOwnership(hx: hx_mod.Hx, err: anyerror) bool {
    switch (err) {
        error.StaleState => hx.fail(ec.ERR_CONNECTOR_STATE_INVALID, S_STATE_STALE),
        error.NotConfigured => hx.fail(ec.ERR_CONNECTOR_NOT_CONFIGURED, "GitHub user authorization is not configured"),
        error.ExchangeFailed => hx.fail(ec.ERR_CONNECTOR_OAUTH_EXCHANGE_FAILED, "GitHub user authorization failed"),
        error.OwnershipDenied => hx.fail(ec.ERR_CONNECTOR_INSTALLATION_OWNERSHIP, "GitHub installation ownership could not be verified"),
        error.DeadlineExceeded, error.SchedulerUnavailable, error.VendorUnreachable => hx.fail(ec.ERR_CONNECTOR_VENDOR_DEADLINE, "GitHub ownership verification did not complete"),
        else => common.internalOperationError(hx.res, "Failed to complete GitHub connection", hx.req_id),
    }
    return false;
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
