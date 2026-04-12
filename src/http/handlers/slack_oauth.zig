// GET /v1/slack/install  — redirect to Slack OAuth consent page.
// GET /v1/slack/callback — exchange code, store token, create routing record.
//
// CSRF state: {nonce_hex}.{workspace_id}.{hmac_hex}
// HMAC input: nonce_hex + ":" + workspace_id  (RULE CTM — constant-time)
// Nonce in Redis (10-min TTL) for replay protection.
//
// Bootstrap exception (§2.0): one confirmation message posted directly after
// OAuth. All subsequent Slack usage routes through M9 execute pipeline.
// Env: SLACK_CLIENT_ID, SLACK_CLIENT_SECRET, SLACK_SIGNING_SECRET

const std = @import("std");
const httpz = @import("httpz");
const HmacSha256 = std.crypto.auth.hmac.sha2.HmacSha256;
const crypto_store = @import("../../secrets/crypto_store.zig");
const workspace_integrations = @import("../../state/workspace_integrations.zig");
const common = @import("common.zig");
const ec = @import("../../errors/error_registry.zig");
const id_format = @import("../../types/id_format.zig");
const oauth_client = @import("slack_oauth_client.zig");

const log = std.log.scoped(.http_slack_oauth);

pub const Context = common.Context;

const SLACK_AUTHORIZE_URL = "https://slack.com/oauth/v2/authorize";
const SLACK_SCOPES = oauth_client.SLACK_SCOPES;
const SLACK_KEY_NAME = "slack";
const NONCE_TTL: u32 = 600;

// ── Install ──────────────────────────────────────────────────────────────────

pub fn handleInstall(ctx: *Context, req: *httpz.Request, res: *httpz.Response) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const client_id = std.process.getEnvVarOwned(alloc, "SLACK_CLIENT_ID") catch {
        common.internalOperationError(res, "SLACK_CLIENT_ID not set", req_id);
        return;
    };
    const secret = std.process.getEnvVarOwned(alloc, "SLACK_SIGNING_SECRET") catch {
        common.internalOperationError(res, "SLACK_SIGNING_SECRET not set", req_id);
        return;
    };
    if (secret.len == 0) { common.internalOperationError(res, "SLACK_SIGNING_SECRET is empty", req_id); return; }
    const qs = req.query() catch {
        common.errorResponse(res, ec.ERR_INVALID_REQUEST, "Invalid query string", req_id);
        return;
    };
    const workspace_id = qs.get("workspace_id") orelse "";

    var nonce_raw: [16]u8 = undefined;
    std.crypto.random.bytes(&nonce_raw);
    const nonce_hex = std.fmt.bytesToHex(nonce_raw, .lower);

    var hmac_out: [HmacSha256.mac_length]u8 = undefined;
    var h = HmacSha256.init(secret);
    h.update(&nonce_hex);
    h.update(":");
    h.update(workspace_id);
    h.final(&hmac_out);
    const hmac_hex = std.fmt.bytesToHex(hmac_out, .lower);

    const state = std.fmt.allocPrint(alloc, "{s}.{s}.{s}", .{ &nonce_hex, workspace_id, &hmac_hex }) catch {
        common.internalOperationError(res, "State build failed", req_id);
        return;
    };

    var nonce_key_buf: [64]u8 = undefined;
    const nonce_key = std.fmt.bufPrint(&nonce_key_buf, "slack:oauth:nonce:{s}", .{&nonce_hex}) catch unreachable;
    ctx.queue.setEx(nonce_key, "1", NONCE_TTL) catch {
        common.internalOperationError(res, "Redis unavailable", req_id);
        return;
    };

    const redir = std.fmt.allocPrint(alloc, "{s}/v1/slack/callback", .{ctx.app_url}) catch {
        common.internalOperationError(res, "Redirect URI overflow", req_id);
        return;
    };
    const redir_enc = oauth_client.urlEncode(alloc, redir) catch {
        common.internalOperationError(res, "URL encode failed", req_id);
        return;
    };
    const location = std.fmt.allocPrint(alloc, "{s}?client_id={s}&scope={s}&state={s}&redirect_uri={s}", .{
        SLACK_AUTHORIZE_URL, client_id, SLACK_SCOPES, state, redir_enc,
    }) catch {
        common.internalOperationError(res, "Location overflow", req_id);
        return;
    };

    res.status = 302;
    res.header("Location", location);
    res.body = "";
    log.info("slack.install workspace_id={s}", .{workspace_id});
}

// ── Callback ─────────────────────────────────────────────────────────────────

pub fn handleCallback(ctx: *Context, req: *httpz.Request, res: *httpz.Response) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const qs = req.query() catch {
        common.errorResponse(res, ec.ERR_INVALID_REQUEST, "Invalid query string", req_id);
        return;
    };
    const code = qs.get("code") orelse {
        if (qs.get("error")) |err| {
            log.warn("slack.callback.denied err={s}", .{err});
            dashboardRedirect(ctx, res, "denied");
            return;
        }
        common.errorResponse(res, ec.ERR_INVALID_REQUEST, "code required", req_id);
        return;
    };
    const state = qs.get("state") orelse {
        common.errorResponse(res, ec.ERR_SLACK_OAUTH_STATE, "state required", req_id);
        return;
    };

    if (!validateState(ctx, alloc, state, res, req_id)) return;

    const ws_in_state = extractWorkspaceId(alloc, state);
    const tok = oauth_client.exchangeCode(alloc, code, ctx.app_url) catch {
        common.errorResponse(res, ec.ERR_SLACK_TOKEN_EXCHANGE, "Slack token exchange failed", req_id);
        return;
    };

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    const workspace_id = if (ws_in_state.len > 0)
        ws_in_state
    else
        id_format.generateWorkspaceId(alloc) catch {
            common.internalOperationError(res, "workspace_id generation failed", req_id);
            return;
        };

    crypto_store.store(alloc, conn, workspace_id, SLACK_KEY_NAME, tok.access_token, 1) catch {
        log.err("slack.callback.vault_fail workspace_id={s}", .{workspace_id});
        common.internalOperationError(res, "Vault store failed", req_id);
        return;
    };

    const upsert = workspace_integrations.upsertIntegration(conn, alloc, workspace_id, "slack", tok.team_id, tok.scope, .oauth) catch {
        common.internalOperationError(res, "Integration record failed", req_id);
        return;
    };
    defer alloc.free(upsert.integration_id);

    // Bootstrap exception — one message direct, all future Slack via M9
    oauth_client.postConfirmation(alloc, tok.access_token) catch |err|
        log.warn("slack.callback.confirm_fail err={s}", .{@errorName(err)});

    log.info("slack.callback.ok workspace_id={s} team_id={s} created={}", .{ workspace_id, tok.team_id, upsert.created });
    dashboardRedirect(ctx, res, "connected");
}

// ── Internal ─────────────────────────────────────────────────────────────────

fn dashboardRedirect(ctx: *Context, res: *httpz.Response, status: []const u8) void {
    var buf: [512]u8 = undefined;
    const loc = std.fmt.bufPrint(&buf, "{s}/dashboard?slack={s}", .{ ctx.app_url, status }) catch "/dashboard";
    res.status = 302;
    res.header("Location", loc);
    res.body = "";
}

fn validateState(ctx: *Context, alloc: std.mem.Allocator, state: []const u8, res: *httpz.Response, req_id: []const u8) bool {
    const secret = std.process.getEnvVarOwned(alloc, "SLACK_SIGNING_SECRET") catch {
        common.internalOperationError(res, "Signing secret not configured", req_id);
        return false;
    };
    const first = std.mem.indexOfScalar(u8, state, '.') orelse {
        common.errorResponse(res, ec.ERR_SLACK_OAUTH_STATE, "Invalid state", req_id);
        return false;
    };
    const last = std.mem.lastIndexOfScalar(u8, state, '.') orelse first;
    if (first == last or state.len < last + 1 + 64) {
        common.errorResponse(res, ec.ERR_SLACK_OAUTH_STATE, "Invalid state", req_id);
        return false;
    }
    const nonce = state[0..first];
    const workspace_id = state[first + 1 .. last];
    const provided = state[last + 1 ..];
    if (provided.len != 64) {
        common.errorResponse(res, ec.ERR_SLACK_OAUTH_STATE, "Invalid state HMAC", req_id);
        return false;
    }

    var expected: [HmacSha256.mac_length]u8 = undefined;
    var hm = HmacSha256.init(secret);
    hm.update(nonce);
    hm.update(":");
    hm.update(workspace_id);
    hm.final(&expected);
    const expected_hex = std.fmt.bytesToHex(expected, .lower);

    var diff: u8 = 0;
    for (provided, &expected_hex) |a, b| diff |= a ^ b;
    if (diff != 0) {
        log.warn("slack.callback.state_mismatch req_id={s}", .{req_id});
        common.errorResponse(res, ec.ERR_SLACK_OAUTH_STATE, "OAuth state invalid", req_id);
        return false;
    }

    var nonce_key_buf: [64]u8 = undefined;
    const nonce_key = std.fmt.bufPrint(&nonce_key_buf, "slack:oauth:nonce:{s}", .{nonce}) catch {
        common.internalOperationError(res, "Nonce key overflow", req_id);
        return false;
    };
    // Atomic consume: DEL returns 1 if the key existed and was deleted, 0 if
    // already gone (expired or replayed). Single command — no TOCTOU window.
    const del_resp = ctx.queue.command(&.{ "DEL", nonce_key }) catch {
        common.internalOperationError(res, "Redis unavailable", req_id);
        return false;
    };
    const consumed = switch (del_resp) {
        .integer => |n| n == 1,
        else => false,
    };
    if (!consumed) {
        common.errorResponse(res, ec.ERR_SLACK_OAUTH_STATE, "OAuth state expired or replayed", req_id);
        return false;
    }
    return true;
}

fn extractWorkspaceId(alloc: std.mem.Allocator, state: []const u8) []const u8 {
    const first = std.mem.indexOfScalar(u8, state, '.') orelse return "";
    const last = std.mem.lastIndexOfScalar(u8, state, '.') orelse return "";
    if (first >= last) return "";
    return alloc.dupe(u8, state[first + 1 .. last]) catch "";
}

// ── Tests ─────────────────────────────────────────────────────────────────────

test "state format: three dot-separated parts" {
    const state = "aabbccddeeff00112233445566778899.ws-001.00000000000000000000000000000000000000000000000000000000000000aa";
    const first = std.mem.indexOfScalar(u8, state, '.').?;
    const last = std.mem.lastIndexOfScalar(u8, state, '.').?;
    try std.testing.expect(first != last);
    try std.testing.expectEqual(@as(usize, 64), state[last + 1 ..].len);
}
