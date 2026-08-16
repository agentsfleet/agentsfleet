//! GitHub callback hook — the provider delta the generic callback handler
//! (`connectors/callback.zig`) dispatches to for the `app_install` archetype.
//! GitHub redirects the operator's browser here after user authorization or an
//! App install. The generic handler has already verified + consumed signed
//! state and resolved the workspace. This hook proves a claimed installation
//! or discovers the unique accessible existing one, then writes the GitHub
//! vault handle the broker mints from.

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
const BindingTxn = @import("../binding_tx.zig");
const spec = @import("spec.zig");
const sql = @import("sql.zig");
const connector_sql = @import("../sql.zig");
const github_connect = @import("connect.zig");
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
const HEADER_LOCATION = "location";
const STATUS_FOUND: u16 = 302;
const CONTINUATION_KEY_PREFIX = "connect:github:continuation:";
const CONTINUATION_KEY_FMT = CONTINUATION_KEY_PREFIX ++ "{s}";
const S_MISSING_CODE = "Missing code";

/// Registry `complete` hook for the app_install archetype. The generic handler
/// has verified the signed state; this hook consumes the latest-state marker
/// adjacent to final persistence.
pub fn complete(hx: hx_mod.Hx, workspace_id: []const u8, raw_state: []const u8, redirect_uri: []const u8, req: *httpz.Request) bool {
    const qs = req.query() catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "Bad query string");
        return false;
    };
    const claimed_installation_id = qs.get(Q_INSTALLATION_ID);
    if (claimed_installation_id) |installation_id| if (!isNumericId(installation_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "Malformed installation_id");
        return false;
    };

    if (qs.get(Q_CODE)) |code| {
        const proof = ownership.resolve(hx, code, claimed_installation_id, redirect_uri) catch |err| return failOwnership(hx, err);
        defer proof.deinit(hx.alloc);

        const installation_id = switch (proof.resolution) {
            .none => return redirectToInstall(hx, workspace_id, raw_state, proof.token),
            .multiple => return failOwnership(hx, error.OwnershipDenied),
            .one => |value| value,
        };

        storeHandle(hx, workspace_id, raw_state, installation_id) catch |err| return failOwnership(hx, err);
    } else {
        const installation_id = claimed_installation_id orelse {
            hx.fail(ec.ERR_INVALID_REQUEST, S_MISSING_CODE);
            return false;
        };
        const token = takeContinuationToken(hx, raw_state) catch |err| {
            if (err == error.ContinuationMissing) {
                hx.fail(ec.ERR_INVALID_REQUEST, S_MISSING_CODE);
                return false;
            }
            return failOwnership(hx, err);
        };
        defer hx.alloc.free(token);
        ownership.verifyClaim(hx, token, installation_id) catch |err| return failOwnership(hx, err);
        storeHandle(hx, workspace_id, raw_state, installation_id) catch |err| return failOwnership(hx, err);
    }

    log.info("github_connected", .{ .workspace_id = workspace_id });
    return true;
}

fn redirectToInstall(hx: hx_mod.Hx, workspace_id: []const u8, raw_state: []const u8, token: []const u8) bool {
    const is_latest = connector_state.consumeLatest(hx.ctx.queue, spec.STATE, workspace_id, raw_state) catch return failOwnership(hx, error.StateVerifyFailed);
    if (!is_latest) return failOwnership(hx, error.StaleState);
    const secret = hx.ctx.approval_signing_secret orelse return failOwnership(hx, error.NotConfigured);
    const starter_subject = hx.principal.user_id orelse return failOwnership(hx, error.StateVerifyFailed);
    const state = connector_state.mint(hx.alloc, hx.ctx.queue, spec.STATE, secret, workspace_id, starter_subject, clock.nowMillis()) catch return failOwnership(hx, error.StateVerifyFailed);
    defer hx.alloc.free(state);
    const url = github_connect.buildInstallUrl(hx, state) catch |err| return failOwnership(hx, err);
    defer hx.alloc.free(url);
    storeContinuationToken(hx, state, token) catch return failOwnership(hx, error.StateVerifyFailed);
    connector_state.markLatest(hx.ctx.queue, spec.STATE, workspace_id, state) catch return failOwnership(hx, error.StateVerifyFailed);
    const location = hx.res.arena.dupe(u8, url) catch return failOwnership(hx, error.OutOfMemory);
    hx.res.status = STATUS_FOUND;
    hx.res.header(HEADER_LOCATION, location);
    hx.res.body = "";
    return false;
}

fn storeContinuationToken(hx: hx_mod.Hx, state: []const u8, token: []const u8) !void {
    const key = try std.fmt.allocPrint(hx.alloc, CONTINUATION_KEY_FMT, .{state});
    defer hx.alloc.free(key);
    try hx.ctx.queue.setEx(key, token, spec.STATE.ttl_seconds);
}

fn takeContinuationToken(hx: hx_mod.Hx, state: []const u8) ![]const u8 {
    const key = try std.fmt.allocPrint(hx.alloc, CONTINUATION_KEY_FMT, .{state});
    defer hx.alloc.free(key);
    var resp = try hx.ctx.queue.command(&.{ "GETDEL", key });
    defer resp.deinit(hx.ctx.queue.alloc);
    return switch (resp) {
        .bulk => |value| hx.alloc.dupe(u8, value orelse return error.ContinuationMissing),
        else => error.StateVerifyFailed,
    };
}

fn storeHandle(hx: hx_mod.Hx, workspace_id: []const u8, raw_state: []const u8, installation_id: []const u8) !void {
    const conn: *pg.Conn = hx.ctx.pool.acquire() catch return error.DbUnavailable;
    defer hx.ctx.pool.release(conn);

    const handle = try std.fmt.allocPrint(hx.alloc, HANDLE_FMT, .{installation_id});
    defer hx.alloc.free(handle);

    const row_id = try id_format.generateConnectorInstallId(hx.alloc);
    defer hx.alloc.free(row_id);
    const no_scopes: []const []const u8 = &.{};
    const now = clock.nowMillis();

    var txn = try BindingTxn.begin(conn, spec.PROVIDER, workspace_id);
    defer txn.abort();
    const is_latest = connector_state.consumeLatest(hx.ctx.queue, spec.STATE, workspace_id, raw_state) catch return error.StateVerifyFailed;
    if (!is_latest) return error.StaleState;
    _ = try conn.exec(connector_sql.DELETE_WORKSPACE_INSTALLS, .{ spec.PROVIDER, workspace_id });
    try vault.storeJsonPlaintext(hx.alloc, conn, workspace_id, spec.PROVIDER, handle);
    {
        var query = PgQuery.from(try conn.query(sql.UPSERT_INSTALL, .{
            row_id,
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
    try txn.commit();
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
