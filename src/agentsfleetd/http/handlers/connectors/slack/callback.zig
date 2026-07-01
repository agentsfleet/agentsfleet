//! GET /v1/connectors/slack/callback?code=&state= — Bearer-less.
//!
//! Slack redirects the admin's browser here after they authorize the app. The
//! signed `state` is the only trust anchor: verify + single-use consume yields
//! the bound workspace, then exchange the `code` for the bot token, vault it as
//! the `fleet:slack` handle (RULE VLT — the token lives only there), record the
//! install in `connector_installs` (for inbound team_id → workspace routing),
//! and 302 back to the dashboard.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const logging = @import("log");
const clock = @import("common").clock;
const common = @import("../../common.zig");
const hx_mod = @import("../../hx.zig");
const ec = @import("../../../../errors/error_registry.zig");
const vault = @import("../../../../state/vault.zig");
const credential_key = @import("../../../../fleet_runtime/credential_key.zig");
const id_format = @import("../../../../types/id_format.zig");
const oauth2 = @import("../oauth2.zig");
const spec = @import("spec.zig");

const log = logging.scoped(.connector_slack);

const Q_CODE = "code";
const Q_STATE = "state";
const CALLBACK_PATH = "/v1/connectors/slack/callback";
const DEST_PATH = "/credentials?connector=slack";
const HEADER_LOCATION = "location";
const STATUS_FOUND: u16 = 302;
const HTTP_OK: u16 = 200;
const S_NOT_CONFIGURED = "Slack connect is not configured";

// Slack `oauth.v2.access` response fields.
const F_OK = "ok";
const F_ACCESS_TOKEN = "access_token";
const F_BOT_USER_ID = "bot_user_id";
const F_SCOPE = "scope";
const F_TEAM = "team";
const F_ID = "id";
const F_NAME = "name";
const F_AUTHED_USER = "authed_user";

const INSERT_INSTALL_SQL =
    \\INSERT INTO core.connector_installs
    \\  (uid, provider, external_account_id, workspace_id, installed_by, scopes, created_at, updated_at)
    \\VALUES ($1::uuid, $2, $3, $4::uuid, $5, $6::text[], $7, $7)
    \\ON CONFLICT (provider, external_account_id) DO UPDATE SET
    \\  workspace_id = EXCLUDED.workspace_id,
    \\  installed_by = EXCLUDED.installed_by,
    \\  scopes = EXCLUDED.scopes,
    \\  updated_at = EXCLUDED.updated_at
;

/// Per-install vault handle (mirrors github's `{integration, …}` shape). Carries
/// the bot token — never an entity-table column (RULE VLT). Stringified with
/// `std.json` so `team_name` (arbitrary workspace name) is safely escaped.
const Handle = struct {
    integration: []const u8,
    bot_token: []const u8,
    bot_user_id: []const u8,
    team_id: []const u8,
    team_name: []const u8,
    scopes: []const u8,
};

const InstallError = error{ NotConfigured, ExchangeFailed } || anyerror;

pub fn innerSlackCallback(hx: hx_mod.Hx, req: *httpz.Request) void {
    const qs = req.query() catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "Bad query string");
        return;
    };
    const raw_state = qs.get(Q_STATE) orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "Missing state");
        return;
    };
    const code = qs.get(Q_CODE) orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "Missing code");
        return;
    };

    const secret = hx.ctx.approval_signing_secret orelse {
        hx.fail(ec.ERR_CONNECTOR_NOT_CONFIGURED, S_NOT_CONFIGURED);
        return;
    };

    const workspace_id = oauth2.consumeState(hx.alloc, hx.ctx.queue, spec.SPEC, secret, raw_state, clock.nowMillis()) orelse {
        hx.fail(ec.ERR_CONNECTOR_STATE_INVALID, "Invalid or expired connect state");
        return;
    };
    defer hx.alloc.free(workspace_id);

    completeInstall(hx, workspace_id, code) catch |err| {
        switch (err) {
            error.NotConfigured => hx.fail(ec.ERR_CONNECTOR_NOT_CONFIGURED, S_NOT_CONFIGURED),
            error.ExchangeFailed => hx.fail(ec.ERR_SLACK_OAUTH_EXCHANGE_FAILED, "Slack token exchange failed"),
            else => common.internalOperationError(hx.res, "Failed to complete Slack connection", hx.req_id),
        }
        return;
    };

    redirectToDashboard(hx);
}

fn completeInstall(hx: hx_mod.Hx, workspace_id: []const u8, code: []const u8) InstallError!void {
    const conn: *pg.Conn = hx.ctx.pool.acquire() catch return error.DbUnavailable;
    defer hx.ctx.pool.release(conn);

    const creds = oauth2.loadAppCreds(hx.alloc, conn, hx.ctx.platform_admin_workspace_id, spec.PROVIDER) orelse return error.NotConfigured;
    defer creds.deinit(hx.alloc);

    const redirect_uri = try joinUrl(hx.alloc, hx.ctx.api_url, CALLBACK_PATH);
    defer hx.alloc.free(redirect_uri);

    const result = try oauth2.exchange(hx.alloc, hx.ctx.io, spec.SPEC, creds, code, redirect_uri);
    defer hx.alloc.free(result.body);
    if (result.status != HTTP_OK) return error.ExchangeFailed;

    var parsed = std.json.parseFromSlice(std.json.Value, hx.alloc, result.body, .{}) catch return error.ExchangeFailed;
    defer parsed.deinit();
    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return error.ExchangeFailed,
    };
    // Borrowed from `parsed` — used before its deinit (the store + insert below).
    const tok = try parseSlackToken(obj);

    try storeHandle(hx, conn, workspace_id, .{
        .integration = spec.PROVIDER,
        .bot_token = tok.bot_token,
        .bot_user_id = tok.bot_user_id,
        .team_id = tok.team_id,
        .team_name = tok.team_name,
        .scopes = tok.scope_csv,
    });
    try insertInstall(hx, conn, workspace_id, tok.team_id, tok.installed_by, tok.scope_csv);

    log.info("slack_connected", .{ .workspace_id = workspace_id, .team_id = tok.team_id });
}

fn storeHandle(hx: hx_mod.Hx, conn: *pg.Conn, workspace_id: []const u8, handle: Handle) !void {
    const key = try credential_key.allocKeyName(hx.alloc, spec.PROVIDER); // "fleet:slack"
    defer hx.alloc.free(key);
    const json = try std.json.Stringify.valueAlloc(hx.alloc, handle, .{});
    defer hx.alloc.free(json);
    try vault.storeJsonPlaintext(hx.alloc, conn, workspace_id, key, json);
}

fn insertInstall(
    hx: hx_mod.Hx,
    conn: *pg.Conn,
    workspace_id: []const u8,
    team_id: []const u8,
    installed_by: []const u8,
    scope_csv: []const u8,
) !void {
    var scopes: std.ArrayList([]const u8) = .empty;
    defer scopes.deinit(hx.alloc);
    var it = std.mem.splitScalar(u8, scope_csv, ',');
    while (it.next()) |s| if (s.len > 0) try scopes.append(hx.alloc, s);

    const uid = try id_format.generateConnectorInstallId(hx.alloc);
    defer hx.alloc.free(uid);
    const now = clock.nowMillis();
    _ = try conn.exec(INSERT_INSTALL_SQL, .{ uid, spec.PROVIDER, team_id, workspace_id, installed_by, scopes.items, now });
}

fn redirectToDashboard(hx: hx_mod.Hx) void {
    const url = joinUrl(hx.alloc, hx.ctx.app_url, DEST_PATH) catch {
        hx.ok(.ok, .{ .status = "connected" }); // connection succeeded; redirect build is cosmetic
        return;
    };
    hx.res.status = STATUS_FOUND;
    hx.res.header(HEADER_LOCATION, url);
    hx.res.body = "";
}

fn joinUrl(alloc: std.mem.Allocator, base: []const u8, path: []const u8) ![]u8 {
    return std.fmt.allocPrint(alloc, "{s}{s}", .{ base, path });
}

fn strField(obj: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    const v = obj.get(key) orelse return null;
    return switch (v) {
        .string => |s| s,
        else => null,
    };
}

fn objField(obj: std.json.ObjectMap, key: []const u8) ?std.json.ObjectMap {
    const v = obj.get(key) orelse return null;
    return switch (v) {
        .object => |o| o,
        else => null,
    };
}

fn nestedStr(obj: std.json.ObjectMap, outer: []const u8, inner: []const u8) ?[]const u8 {
    return strField(objField(obj, outer) orelse return null, inner);
}

fn okTrue(obj: std.json.ObjectMap) bool {
    const v = obj.get(F_OK) orelse return false;
    return v == .bool and v.bool;
}

const SlackToken = struct {
    bot_token: []const u8,
    bot_user_id: []const u8,
    team_id: []const u8,
    team_name: []const u8,
    installed_by: []const u8,
    scope_csv: []const u8,
};

/// Extract the install fields from a Slack `oauth.v2.access` success body. All
/// slices borrow from the caller's parsed JSON (valid until its deinit).
/// `{"ok":false}` or a missing required field → `ExchangeFailed`.
fn parseSlackToken(obj: std.json.ObjectMap) InstallError!SlackToken {
    if (!okTrue(obj)) return error.ExchangeFailed; // {"ok":false,"error":"…"}
    const team = objField(obj, F_TEAM) orelse return error.ExchangeFailed;
    return .{
        .bot_token = strField(obj, F_ACCESS_TOKEN) orelse return error.ExchangeFailed,
        .bot_user_id = strField(obj, F_BOT_USER_ID) orelse return error.ExchangeFailed,
        .team_id = strField(team, F_ID) orelse return error.ExchangeFailed,
        .team_name = strField(team, F_NAME) orelse "",
        .installed_by = nestedStr(obj, F_AUTHED_USER, F_ID) orelse "",
        .scope_csv = strField(obj, F_SCOPE) orelse "",
    };
}

// ── Tests (pure Slack-response parsing; the DB round-trip is integration-gated) ──

const testing = std.testing;

test "parseSlackToken: extracts token + team + installer from an oauth.v2.access success" {
    const body = "{\"ok\":true,\"access_token\":\"xoxb-123\",\"bot_user_id\":\"U9\"," ++
        "\"scope\":\"app_mentions:read,chat:write\",\"team\":{\"id\":\"T024BE7LH\",\"name\":\"Acme Inc\"}," ++
        "\"authed_user\":{\"id\":\"U42\"}}";
    var parsed = try std.json.parseFromSlice(std.json.Value, testing.allocator, body, .{});
    defer parsed.deinit();
    const tok = try parseSlackToken(parsed.value.object);
    try testing.expectEqualStrings("xoxb-123", tok.bot_token);
    try testing.expectEqualStrings("U9", tok.bot_user_id);
    try testing.expectEqualStrings("T024BE7LH", tok.team_id);
    try testing.expectEqualStrings("Acme Inc", tok.team_name);
    try testing.expectEqualStrings("U42", tok.installed_by);
    try testing.expectEqualStrings("app_mentions:read,chat:write", tok.scope_csv);
}

test "parseSlackToken: {ok:false} is rejected" {
    var parsed = try std.json.parseFromSlice(std.json.Value, testing.allocator, "{\"ok\":false,\"error\":\"invalid_code\"}", .{});
    defer parsed.deinit();
    try testing.expectError(error.ExchangeFailed, parseSlackToken(parsed.value.object));
}

test "parseSlackToken: missing access_token is rejected" {
    var parsed = try std.json.parseFromSlice(std.json.Value, testing.allocator, "{\"ok\":true,\"team\":{\"id\":\"T024BE7LH\"}}", .{});
    defer parsed.deinit();
    try testing.expectError(error.ExchangeFailed, parseSlackToken(parsed.value.object));
}
