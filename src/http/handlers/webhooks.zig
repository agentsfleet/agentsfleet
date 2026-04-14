// POST /v1/webhooks/{zombie_id} or /v1/webhooks/{zombie_id}/{secret}
//
// Auth: URL-embedded secret (resolved from vault via webhook_secret_ref)
//       or Bearer token (config_json->'trigger'->>'token') as fallback.
// Idempotency: Redis SET NX EX on "webhook:dedup:{zombie_id}:{event_id}".
// On success: event enqueued to zombie:{zombie_id}:events stream, returns 202.
//
// none policy — auth is via URL-embedded secret or trigger token, not Bearer.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const common = @import("common.zig");
const hx_mod = @import("hx.zig");
const ec = @import("../../errors/error_registry.zig");
const zombie_config = @import("../../zombie/config.zig");
const activity_stream = @import("../../zombie/activity_stream.zig");
const crypto_store = @import("../../secrets/crypto_store.zig");
const telemetry_mod = @import("../../observability/telemetry.zig");
const metrics_counters = @import("../../observability/metrics_counters.zig");

const log = std.log.scoped(.http_webhook);

pub const Context = common.Context;
const Hx = hx_mod.Hx;

const WebhookPayload = struct {
    event_id: []const u8,
    type: []const u8,
    data: std.json.Value,
};

const ZombieRow = struct {
    workspace_id: []const u8,
    status: []const u8,
    token: ?[]const u8,
    webhook_secret_ref: ?[]const u8,
    source: ?[]const u8,
};

fn deinitZombieRow(row: *const ZombieRow, alloc: std.mem.Allocator) void {
    alloc.free(row.workspace_id);
    alloc.free(row.status);
    if (row.token) |t| alloc.free(t);
    if (row.webhook_secret_ref) |s| alloc.free(s);
    if (row.source) |s| alloc.free(s);
}

fn fetchZombieById(pool: *pg.Pool, alloc: std.mem.Allocator, zombie_id: []const u8) !?ZombieRow {
    const conn = try pool.acquire();
    defer pool.release(conn);
    var q = PgQuery.from(try conn.query(
        \\SELECT workspace_id::text, status,
        \\       config_json->'trigger'->>'token',
        \\       webhook_secret_ref,
        \\       config_json->'trigger'->>'source'
        \\FROM core.zombies WHERE id = $1::uuid
    , .{zombie_id}));
    defer q.deinit();
    const row = try q.next() orelse return null;
    const workspace_id = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(workspace_id);
    const status = try alloc.dupe(u8, try row.get([]const u8, 1));
    errdefer alloc.free(status);
    const token_raw = row.get([]const u8, 2) catch null;
    const token: ?[]const u8 = if (token_raw) |t| try alloc.dupe(u8, t) else null;
    errdefer if (token) |t| alloc.free(t);
    const ref_raw = row.get([]const u8, 3) catch null;
    const webhook_secret_ref: ?[]const u8 = if (ref_raw) |s| try alloc.dupe(u8, s) else null;
    errdefer if (webhook_secret_ref) |s| alloc.free(s);
    const source_raw = row.get([]const u8, 4) catch null;
    const source: ?[]const u8 = if (source_raw) |s| try alloc.dupe(u8, s) else null;
    return .{ .workspace_id = workspace_id, .status = status, .token = token, .webhook_secret_ref = webhook_secret_ref, .source = source };
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn parseBody(hx: Hx, req: *httpz.Request, zombie_id: []const u8) ?WebhookPayload {
    const body = req.body() orelse {
        log.warn("webhook.no_body zombie_id={s} req_id={s}", .{ zombie_id, hx.req_id });
        hx.fail(ec.ERR_WEBHOOK_MALFORMED, ec.MSG_BODY_REQUIRED);
        return null;
    };
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return null;
    const parsed = std.json.parseFromSlice(WebhookPayload, hx.alloc, body, .{ .ignore_unknown_fields = true }) catch {
        log.warn("webhook.malformed_json zombie_id={s} req_id={s}", .{ zombie_id, hx.req_id });
        hx.fail(ec.ERR_WEBHOOK_MALFORMED, ec.MSG_MALFORMED_JSON);
        return null;
    };
    const payload = parsed.value;
    if (payload.event_id.len == 0 or payload.type.len == 0) {
        hx.fail(ec.ERR_WEBHOOK_MALFORMED, ec.MSG_MISSING_FIELDS);
        parsed.deinit();
        return null;
    }
    return payload;
}

// M2_002: Unified webhook auth — URL secret (vault-backed) first, Bearer token fallback.
fn verifyWebhookAuth(hx: Hx, url_secret: ?[]const u8, zombie: *const ZombieRow, req: *httpz.Request) bool {
    // Path 1: URL-embedded secret — resolve from vault via webhook_secret_ref
    if (url_secret) |provided_secret| {
        const ref = zombie.webhook_secret_ref orelse {
            hx.fail(ec.ERR_UNAUTHORIZED, ec.MSG_AUTH_REQUIRED);
            return false;
        };
        const conn = hx.ctx.pool.acquire() catch {
            log.err("webhook.vault_pool_acquire_failed req_id={s}", .{hx.req_id});
            hx.fail(ec.ERR_UNAUTHORIZED, ec.MSG_AUTH_REQUIRED);
            return false;
        };
        defer hx.ctx.pool.release(conn);
        const expected = crypto_store.load(hx.alloc, conn, zombie.workspace_id, ref) catch {
            log.err("webhook.vault_load_failed ref={s} req_id={s}", .{ ref, hx.req_id });
            hx.fail(ec.ERR_UNAUTHORIZED, ec.MSG_AUTH_REQUIRED);
            return false;
        };
        defer hx.alloc.free(expected);
        return constantTimeEq(hx, provided_secret, expected);
    }
    // Path 2: Bearer token fallback
    const expected_token = zombie.token orelse {
        hx.fail(ec.ERR_UNAUTHORIZED, ec.MSG_AUTH_REQUIRED);
        return false;
    };
    return verifyBearerToken(hx, expected_token, req);
}

fn constantTimeEq(hx: Hx, a: []const u8, b: []const u8) bool {
    // Always run XOR loop over min length to avoid leaking secret length via timing.
    const min_len = @min(a.len, b.len);
    var diff: u8 = 0;
    for (a[0..min_len], b[0..min_len]) |x, y| diff |= x ^ y;
    // Length mismatch is not secret (attacker controls input length), but fold it
    // into the result after the constant-time loop to prevent short-circuit.
    diff |= @as(u8, @intFromBool(a.len != b.len));
    if (diff != 0) {
        hx.fail(ec.ERR_UNAUTHORIZED, ec.MSG_INVALID_TOKEN);
        return false;
    }
    return true;
}

fn verifyBearerToken(hx: Hx, expected_token: []const u8, req: *httpz.Request) bool {
    const auth_header = req.header("authorization") orelse {
        hx.fail(ec.ERR_UNAUTHORIZED, ec.MSG_AUTH_REQUIRED);
        return false;
    };
    if (!std.mem.startsWith(u8, auth_header, ec.BEARER_PREFIX)) {
        hx.fail(ec.ERR_UNAUTHORIZED, ec.MSG_BEARER_REQUIRED);
        return false;
    }
    const provided = auth_header[ec.BEARER_PREFIX.len..];
    const valid = provided.len == expected_token.len and ct: {
        var diff: u8 = 0;
        for (provided, expected_token) |a, b| diff |= a ^ b;
        break :ct diff == 0;
    };
    if (!valid) {
        hx.fail(ec.ERR_UNAUTHORIZED, ec.MSG_INVALID_TOKEN);
        return false;
    }
    return true;
}

fn dedupAndEnqueue(hx: Hx, zombie_id: []const u8, payload: WebhookPayload, source_label: []const u8) bool {
    var dedup_key_buf: [256]u8 = undefined;
    const dedup_key = std.fmt.bufPrint(&dedup_key_buf, "webhook:dedup:{s}:{s}", .{ zombie_id, payload.event_id }) catch {
        common.internalOperationError(hx.res, "dedup key overflow", hx.req_id);
        return false;
    };
    const is_new = hx.ctx.queue.setNx(dedup_key, "1", ec.DEDUP_TTL_SECONDS) catch |err| {
        log.err("webhook.redis_dedup_error zombie_id={s} event_id={s} err={s}", .{ zombie_id, payload.event_id, @errorName(err) });
        common.internalOperationError(hx.res, "Idempotency check failed", hx.req_id);
        return false;
    };
    if (!is_new) {
        log.debug("webhook.duplicate zombie_id={s} event_id={s}", .{ zombie_id, payload.event_id });
        hx.ok(.ok, .{ .status = ec.STATUS_DUPLICATE });
        return false;
    }
    const data_json = std.fmt.allocPrint(hx.alloc, "{f}", .{std.json.fmt(payload.data, .{})}) catch {
        common.internalOperationError(hx.res, "Failed to serialize event data", hx.req_id);
        return false;
    };
    hx.ctx.queue.xaddZombieEvent(zombie_id, payload.event_id, payload.type, source_label, data_json) catch |err| {
        log.err("webhook.enqueue_failed zombie_id={s} event_id={s} err={s}", .{ zombie_id, payload.event_id, @errorName(err) });
        common.internalOperationError(hx.res, "Failed to enqueue event", hx.req_id);
        return false;
    };
    return true;
}

// ── Main handler ───────────────────────────────────────────────────────────

pub fn innerReceiveWebhook(hx: Hx, req: *httpz.Request, zombie_id: []const u8, url_secret: ?[]const u8) void {
    const payload = parseBody(hx, req, zombie_id) orelse return;

    var zombie = fetchZombieById(hx.ctx.pool, hx.alloc, zombie_id) catch |err| {
        log.err("webhook.db_error zombie_id={s} err={s} req_id={s}", .{ zombie_id, @errorName(err), hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    } orelse {
        log.warn("webhook.not_found zombie_id={s} req_id={s}", .{ zombie_id, hx.req_id });
        hx.fail(ec.ERR_WEBHOOK_NO_ZOMBIE, ec.MSG_ZOMBIE_NOT_FOUND);
        return;
    };
    defer deinitZombieRow(&zombie, hx.alloc);

    // M2_002: URL secret (vault-backed via webhook_secret_ref) or Bearer token fallback.
    if (!verifyWebhookAuth(hx, url_secret, &zombie, req)) return;

    const status = zombie_config.ZombieStatus.fromSlice(zombie.status) orelse .stopped;
    if (!status.isRunnable()) {
        log.warn("webhook.zombie_not_active zombie_id={s} status={s} req_id={s}", .{ zombie_id, zombie.status, hx.req_id });
        hx.fail(ec.ERR_WEBHOOK_ZOMBIE_PAUSED, ec.MSG_ZOMBIE_NOT_ACTIVE);
        return;
    }

    const source_label = zombie.source orelse "";
    if (!dedupAndEnqueue(hx, zombie_id, payload, source_label)) return;

    activity_stream.logEvent(hx.ctx.pool, hx.alloc, .{
        .zombie_id = zombie_id,
        .workspace_id = zombie.workspace_id,
        .event_type = ec.WEBHOOK_EVENT_TYPE,
        .detail = payload.event_id,
    });

    recordWebhookAccepted(hx.ctx.telemetry, zombie.workspace_id, zombie_id, payload.event_id, source_label);

    log.info("webhook.accepted zombie_id={s} event_id={s} type={s}", .{ zombie_id, payload.event_id, payload.type });
    hx.ok(.accepted, .{ .status = ec.STATUS_ACCEPTED, .event_id = payload.event_id });
}

/// Record observability for a successfully accepted webhook (202 path).
/// Increments the Prometheus triggered counter and emits a PostHog event.
/// Webhook path has no authenticated user — distinct_id is workspace_id so
/// zombie events group under the owning workspace in funnels/retention.
fn recordWebhookAccepted(
    tel: *telemetry_mod.Telemetry,
    workspace_id: []const u8,
    zombie_id: []const u8,
    event_id: []const u8,
    source: []const u8,
) void {
    metrics_counters.incZombiesTriggered();
    tel.capture(telemetry_mod.ZombieTriggered, .{
        .distinct_id = workspace_id,
        .workspace_id = workspace_id,
        .zombie_id = zombie_id,
        .event_id = event_id,
        .source = source,
    });
}

// Spec §6.0 row 3.1 — webhook_increments_triggered: successful 202 path increments
// zombies_triggered_total and emits zombie_triggered PostHog event.
test "M15_002 3.1: webhook_increments_triggered" {
    const metrics_zombie = @import("../../observability/metrics_zombie.zig");
    const tel_mod = @import("../../observability/telemetry.zig");
    metrics_zombie.resetForTest();
    defer metrics_zombie.resetForTest();
    var tel = tel_mod.Telemetry.initTest();
    const before = metrics_zombie.snapshotZombieFields().zombie_triggered_total;
    recordWebhookAccepted(&tel, "ws_001", "z_001", "evt_001", "webhook");
    try std.testing.expectEqual(@as(u64, 1), metrics_zombie.snapshotZombieFields().zombie_triggered_total - before);
    try tel_mod.TestBackend.assertLastEventIs(.zombie_triggered);
}

test "WebhookPayload parses valid event" {
    const alloc = std.testing.allocator;
    const body =
        \\{"event_id":"evt_001","type":"email.received","data":{"from":"a@b.com"}}
    ;
    const parsed = try std.json.parseFromSlice(WebhookPayload, alloc, body, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    try std.testing.expectEqualStrings("evt_001", parsed.value.event_id);
    try std.testing.expectEqualStrings("email.received", parsed.value.type);
}

test "WebhookPayload rejects missing event_id" {
    const alloc = std.testing.allocator;
    const body =
        \\{"type":"email.received","data":{}}
    ;
    const result = std.json.parseFromSlice(WebhookPayload, alloc, body, .{});
    try std.testing.expect(if (result) |_| false else |_| true);
}
