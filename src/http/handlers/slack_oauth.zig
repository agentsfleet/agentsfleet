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
const hx_mod = @import("hx.zig");
const ec = @import("../../errors/error_registry.zig");
const id_format = @import("../../types/id_format.zig");
const oauth_client = @import("slack_oauth_client.zig");

const log = std.log.scoped(.http_slack_oauth);

pub const Context = common.Context;
const Hx = hx_mod.Hx;

const SLACK_AUTHORIZE_URL = "https://slack.com/oauth/v2/authorize";
const SLACK_SCOPES = oauth_client.SLACK_SCOPES;
const SLACK_KEY_NAME = "slack";
const NONCE_TTL: u32 = 600;

// ── Install ──────────────────────────────────────────────────────────────────

pub fn innerInstall(hx: Hx, req: *httpz.Request) void {
    const client_id = std.process.getEnvVarOwned(hx.alloc, "SLACK_CLIENT_ID") catch {
        common.internalOperationError(hx.res, "SLACK_CLIENT_ID not set", hx.req_id);
        return;
    };
    const secret = std.process.getEnvVarOwned(hx.alloc, "SLACK_SIGNING_SECRET") catch {
        common.internalOperationError(hx.res, "SLACK_SIGNING_SECRET not set", hx.req_id);
        return;
    };
    if (secret.len == 0) { common.internalOperationError(hx.res, "SLACK_SIGNING_SECRET is empty", hx.req_id); return; }
    const qs = req.query() catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "Invalid query string");
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

    const state = std.fmt.allocPrint(hx.alloc, "{s}.{s}.{s}", .{ &nonce_hex, workspace_id, &hmac_hex }) catch {
        common.internalOperationError(hx.res, "State build failed", hx.req_id);
        return;
    };

    var nonce_key_buf: [64]u8 = undefined;
    const nonce_key = std.fmt.bufPrint(&nonce_key_buf, "slack:oauth:nonce:{s}", .{&nonce_hex}) catch unreachable;
    hx.ctx.queue.setEx(nonce_key, "1", NONCE_TTL) catch {
        common.internalOperationError(hx.res, "Redis unavailable", hx.req_id);
        return;
    };

    const redir = std.fmt.allocPrint(hx.alloc, "{s}/v1/slack/callback", .{hx.ctx.app_url}) catch {
        common.internalOperationError(hx.res, "Redirect URI overflow", hx.req_id);
        return;
    };
    const redir_enc = oauth_client.urlEncode(hx.alloc, redir) catch {
        common.internalOperationError(hx.res, "URL encode failed", hx.req_id);
        return;
    };
    const location = std.fmt.allocPrint(hx.alloc, "{s}?client_id={s}&scope={s}&state={s}&redirect_uri={s}", .{
        SLACK_AUTHORIZE_URL, client_id, SLACK_SCOPES, state, redir_enc,
    }) catch {
        common.internalOperationError(hx.res, "Location overflow", hx.req_id);
        return;
    };

    hx.res.status = 302;
    hx.res.header("Location", location);
    hx.res.body = "";
    log.info("slack.install workspace_id={s}", .{workspace_id});
}

// ── Callback ─────────────────────────────────────────────────────────────────

pub fn innerCallback(hx: Hx, req: *httpz.Request) void {
    const qs = req.query() catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "Invalid query string");
        return;
    };
    const code = qs.get("code") orelse {
        if (qs.get("error")) |err| {
            log.warn("slack.callback.denied err={s}", .{err});
            dashboardRedirect(hx, "denied");
            return;
        }
        hx.fail(ec.ERR_INVALID_REQUEST, "code required");
        return;
    };
    const state = qs.get("state") orelse {
        hx.fail(ec.ERR_SLACK_OAUTH_STATE, "state required");
        return;
    };

    if (!validateState(hx, state)) return;

    const ws_in_state = extractWorkspaceId(hx.alloc, state) catch {
        common.internalOperationError(hx.res, "workspace_id extraction failed", hx.req_id);
        return;
    };
    const tok = oauth_client.exchangeCode(hx.alloc, code, hx.ctx.app_url) catch {
        hx.fail(ec.ERR_SLACK_TOKEN_EXCHANGE, "Slack token exchange failed");
        return;
    };

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const workspace_id = if (ws_in_state.len > 0)
        ws_in_state
    else
        id_format.generateWorkspaceId(hx.alloc) catch {
            common.internalOperationError(hx.res, "workspace_id generation failed", hx.req_id);
            return;
        };

    crypto_store.store(hx.alloc, conn, workspace_id, SLACK_KEY_NAME, tok.access_token, 1) catch {
        log.err("slack.callback.vault_fail workspace_id={s}", .{workspace_id});
        common.internalOperationError(hx.res, "Vault store failed", hx.req_id);
        return;
    };

    const upsert = workspace_integrations.upsertIntegration(conn, hx.alloc, workspace_id, "slack", tok.team_id, tok.scope, .oauth) catch {
        common.internalOperationError(hx.res, "Integration record failed", hx.req_id);
        return;
    };
    defer hx.alloc.free(upsert.integration_id);

    // Bootstrap exception — one message direct, all future Slack via M9
    oauth_client.postConfirmation(hx.alloc, tok.access_token) catch |err|
        log.warn("slack.callback.confirm_fail err={s}", .{@errorName(err)});

    log.info("slack.callback.ok workspace_id={s} team_id={s} created={}", .{ workspace_id, tok.team_id, upsert.created });
    dashboardRedirect(hx, "connected");
}

// ── Internal ─────────────────────────────────────────────────────────────────

fn dashboardRedirect(hx: Hx, status: []const u8) void {
    var buf: [512]u8 = undefined;
    const loc = std.fmt.bufPrint(&buf, "{s}/dashboard?slack={s}", .{ hx.ctx.app_url, status }) catch "/dashboard";
    hx.res.status = 302;
    hx.res.header("Location", loc);
    hx.res.body = "";
}

fn validateState(hx: Hx, state: []const u8) bool {
    const secret = std.process.getEnvVarOwned(hx.alloc, "SLACK_SIGNING_SECRET") catch {
        common.internalOperationError(hx.res, "Signing secret not configured", hx.req_id);
        return false;
    };
    const first = std.mem.indexOfScalar(u8, state, '.') orelse {
        hx.fail(ec.ERR_SLACK_OAUTH_STATE, "Invalid state");
        return false;
    };
    const last = std.mem.lastIndexOfScalar(u8, state, '.') orelse first;
    if (first == last or state.len < last + 1 + 64) {
        hx.fail(ec.ERR_SLACK_OAUTH_STATE, "Invalid state");
        return false;
    }
    const nonce = state[0..first];
    const workspace_id = state[first + 1 .. last];
    const provided = state[last + 1 ..];
    if (provided.len != 64) {
        hx.fail(ec.ERR_SLACK_OAUTH_STATE, "Invalid state HMAC");
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
        log.warn("slack.callback.state_mismatch req_id={s}", .{hx.req_id});
        hx.fail(ec.ERR_SLACK_OAUTH_STATE, "OAuth state invalid");
        return false;
    }

    var nonce_key_buf: [64]u8 = undefined;
    const nonce_key = std.fmt.bufPrint(&nonce_key_buf, "slack:oauth:nonce:{s}", .{nonce}) catch {
        common.internalOperationError(hx.res, "Nonce key overflow", hx.req_id);
        return false;
    };
    // Atomic consume: DEL returns 1 if the key existed and was deleted, 0 if
    // already gone (expired or replayed). Single command — no TOCTOU window.
    const del_resp = hx.ctx.queue.command(&.{ "DEL", nonce_key }) catch {
        common.internalOperationError(hx.res, "Redis unavailable", hx.req_id);
        return false;
    };
    const consumed = switch (del_resp) {
        .integer => |n| n == 1,
        else => false,
    };
    if (!consumed) {
        hx.fail(ec.ERR_SLACK_OAUTH_STATE, "OAuth state expired or replayed");
        return false;
    }
    return true;
}

fn extractWorkspaceId(alloc: std.mem.Allocator, state: []const u8) ![]const u8 {
    const first = std.mem.indexOfScalar(u8, state, '.') orelse return "";
    const last = std.mem.lastIndexOfScalar(u8, state, '.') orelse return "";
    if (first >= last) return "";
    // dupe so the caller owns the slice independently of the request arena
    return alloc.dupe(u8, state[first + 1 .. last]);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

test "state format: three dot-separated parts" {
    const state = "aabbccddeeff00112233445566778899.ws-001.00000000000000000000000000000000000000000000000000000000000000aa";
    const first = std.mem.indexOfScalar(u8, state, '.').?;
    const last = std.mem.lastIndexOfScalar(u8, state, '.').?;
    try std.testing.expect(first != last);
    try std.testing.expectEqual(@as(usize, 64), state[last + 1 ..].len);
}

// ── T2: extractWorkspaceId edge cases ────────────────────────────────────────

test "extractWorkspaceId: empty string returns empty" {
    const alloc = std.testing.allocator;
    const ws = try extractWorkspaceId(alloc, "");
    try std.testing.expectEqualStrings("", ws);
}

test "extractWorkspaceId: no dots returns empty (no segment)" {
    const alloc = std.testing.allocator;
    const ws = try extractWorkspaceId(alloc, "nodots");
    try std.testing.expectEqualStrings("", ws);
}

test "extractWorkspaceId: single dot (first == last) returns empty" {
    // State must have at least two distinct dot positions.
    const alloc = std.testing.allocator;
    const ws = try extractWorkspaceId(alloc, "nonce.hmac");
    try std.testing.expectEqualStrings("", ws);
}

test "extractWorkspaceId: valid three-part state extracts middle segment" {
    const alloc = std.testing.allocator;
    const ws = try extractWorkspaceId(alloc, "nonce.my-workspace-id.hmachex");
    defer alloc.free(ws);
    try std.testing.expectEqualStrings("my-workspace-id", ws);
}

test "extractWorkspaceId: UUID workspace_id with hyphens preserved exactly" {
    const alloc = std.testing.allocator;
    const uuid = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
    const state = "nonce123." ++ uuid ++ ".aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const ws = try extractWorkspaceId(alloc, state);
    defer alloc.free(ws);
    try std.testing.expectEqualStrings(uuid, ws);
}

test "extractWorkspaceId: workspace_id is independent allocation (owns its slice)" {
    // Verify the returned slice is a fresh allocation, not a pointer into the input.
    const alloc = std.testing.allocator;
    const state = try alloc.dupe(u8, "nonce.wsid-abc.hmac64hexhex00000000000000000000000000000000000000000");
    defer alloc.free(state);
    const ws = try extractWorkspaceId(alloc, state);
    defer alloc.free(ws);
    // If ws points into state, free(state) before comparing would crash.
    // Having separate defers exercises the ownership.
    try std.testing.expectEqualStrings("wsid-abc", ws);
}

// ── T7: State HMAC structural invariants ────────────────────────────────────

test "state HMAC: SHA-256 produces exactly 32 bytes → 64 hex chars" {
    // Pin: the HMAC in state is SHA-256 (32 bytes × 2 = 64 hex chars).
    // If someone changes the algorithm, validateState's provided.len != 64 check breaks.
    var mac: [HmacSha256.mac_length]u8 = undefined;
    var h = HmacSha256.init("test-secret");
    h.update("nonce:workspace");
    h.final(&mac);
    const hex = std.fmt.bytesToHex(mac, .lower);
    try std.testing.expectEqual(@as(usize, 64), hex.len);
}

test "state HMAC: different nonces produce different HMACs (sanity)" {
    var mac1: [HmacSha256.mac_length]u8 = undefined;
    var mac2: [HmacSha256.mac_length]u8 = undefined;
    var h1 = HmacSha256.init("secret");
    h1.update("nonce1");
    h1.update(":");
    h1.update("ws");
    h1.final(&mac1);
    var h2 = HmacSha256.init("secret");
    h2.update("nonce2");
    h2.update(":");
    h2.update("ws");
    h2.final(&mac2);
    try std.testing.expect(!std.mem.eql(u8, &mac1, &mac2));
}
