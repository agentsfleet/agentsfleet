// POST /v1/webhooks/{zombie_id} — inbound event delivery for Zombies.
//
// Auth: Bearer token matched against core.zombies.config_json->'trigger'->>'token'.
// Idempotency: Redis SET NX EX on "webhook:dedup:{zombie_id}:{event_id}".
// On success: event enqueued to zombie:{zombie_id}:events stream, returns 202.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const common = @import("common.zig");
const ec = @import("../../errors/codes.zig");
const zombie_config = @import("../../zombie/config.zig");
const activity_stream = @import("../../zombie/activity_stream.zig");

const log = std.log.scoped(.http_webhook);

pub const Context = common.Context;

const WebhookPayload = struct {
    event_id: []const u8,
    @"type": []const u8,
    data: std.json.Value,
};

const ZombieRow = struct {
    workspace_id: []u8,
    status: []u8,
    token: ?[]u8,
    source: ?[]u8,
};

fn deinitZombieRow(row: *ZombieRow, alloc: std.mem.Allocator) void {
    alloc.free(row.workspace_id);
    alloc.free(row.status);
    if (row.token) |t| alloc.free(t);
    if (row.source) |s| alloc.free(s);
}

fn fetchZombieById(pool: *pg.Pool, alloc: std.mem.Allocator, zombie_id: []const u8) !?ZombieRow {
    const conn = try pool.acquire();
    defer pool.release(conn);
    var q = try conn.query( // check-pg-drain: ok — drain called below
        \\SELECT workspace_id, status,
        \\       config_json->'trigger'->>'token',
        \\       config_json->'trigger'->>'source'
        \\FROM core.zombies WHERE id = $1::uuid
    , .{zombie_id});
    defer q.deinit();
    const row = try q.next() orelse {
        q.drain() catch {};
        return null;
    };
    const workspace_id = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(workspace_id);
    const status = try alloc.dupe(u8, try row.get([]const u8, 1));
    errdefer alloc.free(status);
    const token_raw = row.get([]const u8, 2) catch null;
    const token: ?[]u8 = if (token_raw) |t| try alloc.dupe(u8, t) else null;
    errdefer if (token) |t| alloc.free(t);
    const source_raw = row.get([]const u8, 3) catch null;
    const source: ?[]u8 = if (source_raw) |s| try alloc.dupe(u8, s) else null;
    q.drain() catch {};
    return .{ .workspace_id = workspace_id, .status = status, .token = token, .source = source };
}

// ── Helpers (each ≤ 20 lines) ──────────────────────────────────────────────

fn parseBody(alloc: std.mem.Allocator, req: *httpz.Request, res: *httpz.Response, zombie_id: []const u8, req_id: []const u8) ?WebhookPayload {
    const body = req.body() orelse {
        log.warn("webhook.no_body zombie_id={s} req_id={s}", .{ zombie_id, req_id });
        common.errorResponse(res, .bad_request, ec.ERR_WEBHOOK_MALFORMED, ec.MSG_BODY_REQUIRED, req_id);
        return null;
    };
    if (!common.checkBodySize(req, res, body, req_id)) return null;
    const parsed = std.json.parseFromSlice(WebhookPayload, alloc, body, .{ .ignore_unknown_fields = true }) catch {
        log.warn("webhook.malformed_json zombie_id={s} req_id={s}", .{ zombie_id, req_id });
        common.errorResponse(res, .bad_request, ec.ERR_WEBHOOK_MALFORMED, ec.MSG_MALFORMED_JSON, req_id);
        return null;
    };
    const payload = parsed.value;
    if (payload.event_id.len == 0 or payload.@"type".len == 0) {
        common.errorResponse(res, .bad_request, ec.ERR_WEBHOOK_MALFORMED, ec.MSG_MISSING_FIELDS, req_id);
        parsed.deinit();
        return null;
    }
    return payload;
}

fn verifyBearerToken(expected_token: []const u8, req: *httpz.Request, res: *httpz.Response, req_id: []const u8) bool {
    const auth_header = req.header("authorization") orelse {
        common.errorResponse(res, .unauthorized, ec.ERR_FORBIDDEN, ec.MSG_AUTH_REQUIRED, req_id);
        return false;
    };
    if (!std.mem.startsWith(u8, auth_header, ec.BEARER_PREFIX)) {
        common.errorResponse(res, .unauthorized, ec.ERR_FORBIDDEN, ec.MSG_BEARER_REQUIRED, req_id);
        return false;
    }
    const provided = auth_header[ec.BEARER_PREFIX.len..];
    const valid = provided.len == expected_token.len and ct: {
        var diff: u8 = 0;
        for (provided, expected_token) |a, b| diff |= a ^ b;
        break :ct diff == 0;
    };
    if (!valid) {
        common.errorResponse(res, .unauthorized, ec.ERR_FORBIDDEN, ec.MSG_INVALID_TOKEN, req_id);
        return false;
    }
    return true;
}

fn dedupAndEnqueue(ctx: *Context, alloc: std.mem.Allocator, res: *httpz.Response, zombie_id: []const u8, payload: WebhookPayload, source_label: []const u8, req_id: []const u8) bool {
    var dedup_key_buf: [256]u8 = undefined;
    const dedup_key = std.fmt.bufPrint(&dedup_key_buf, "webhook:dedup:{s}:{s}", .{ zombie_id, payload.event_id }) catch {
        common.internalOperationError(res, "dedup key overflow", req_id);
        return false;
    };
    const is_new = ctx.queue.setNx(dedup_key, "1", ec.DEDUP_TTL_SECONDS) catch |err| {
        log.err("webhook.redis_dedup_error zombie_id={s} event_id={s} err={s}", .{ zombie_id, payload.event_id, @errorName(err) });
        common.internalOperationError(res, "Idempotency check failed", req_id);
        return false;
    };
    if (!is_new) {
        log.debug("webhook.duplicate zombie_id={s} event_id={s}", .{ zombie_id, payload.event_id });
        res.status = 200;
        res.json(.{ .status = ec.STATUS_DUPLICATE }, .{}) catch {};
        return false;
    }
    const data_json = std.fmt.allocPrint(alloc, "{f}", .{std.json.fmt(payload.data, .{})}) catch {
        common.internalOperationError(res, "Failed to serialize event data", req_id);
        return false;
    };
    ctx.queue.xaddZombieEvent(zombie_id, payload.event_id, payload.@"type", source_label, data_json) catch |err| {
        log.err("webhook.enqueue_failed zombie_id={s} event_id={s} err={s}", .{ zombie_id, payload.event_id, @errorName(err) });
        common.internalOperationError(res, "Failed to enqueue event", req_id);
        return false;
    };
    return true;
}

// ── Main handler ───────────────────────────────────────────────────────────

pub fn handleReceiveWebhook(ctx: *Context, req: *httpz.Request, res: *httpz.Response, zombie_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const payload = parseBody(alloc, req, res, zombie_id, req_id) orelse return;

    var zombie = fetchZombieById(ctx.pool, alloc, zombie_id) catch |err| {
        log.err("webhook.db_error zombie_id={s} err={s} req_id={s}", .{ zombie_id, @errorName(err), req_id });
        common.internalDbError(res, req_id);
        return;
    } orelse {
        log.warn("webhook.not_found zombie_id={s} req_id={s}", .{ zombie_id, req_id });
        common.errorResponse(res, .not_found, ec.ERR_WEBHOOK_NO_ZOMBIE, ec.MSG_ZOMBIE_NOT_FOUND, req_id);
        return;
    };
    defer deinitZombieRow(&zombie, alloc);

    // Auth: Bearer token is always required. Token is generated at deploy time
    // and stored in config_json. No token = reject (zombiectl up must set one).
    const expected_token = zombie.token orelse {
        log.warn("webhook.no_token_configured zombie_id={s} req_id={s}", .{ zombie_id, req_id });
        common.errorResponse(res, .forbidden, ec.ERR_FORBIDDEN, ec.MSG_AUTH_REQUIRED, req_id);
        return;
    };
    if (!verifyBearerToken(expected_token, req, res, req_id)) return;

    const status = zombie_config.ZombieStatus.fromSlice(zombie.status) orelse .stopped;
    if (!status.isRunnable()) {
        log.warn("webhook.zombie_not_active zombie_id={s} status={s} req_id={s}", .{ zombie_id, zombie.status, req_id });
        common.errorResponse(res, .conflict, ec.ERR_WEBHOOK_ZOMBIE_PAUSED, ec.MSG_ZOMBIE_NOT_ACTIVE, req_id);
        return;
    }

    const source_label = zombie.source orelse "";
    if (!dedupAndEnqueue(ctx, alloc, res, zombie_id, payload, source_label, req_id)) return;

    activity_stream.logEvent(ctx.pool, alloc, .{
        .zombie_id = zombie_id,
        .workspace_id = zombie.workspace_id,
        .event_type = ec.WEBHOOK_EVENT_TYPE,
        .detail = payload.event_id,
    });

    log.info("webhook.accepted zombie_id={s} event_id={s} type={s}", .{ zombie_id, payload.event_id, payload.@"type" });
    res.status = 202;
    res.json(.{ .status = ec.STATUS_ACCEPTED, .event_id = payload.event_id }, .{}) catch {};
}

test "WebhookPayload parses valid event" {
    const alloc = std.testing.allocator;
    const body =
        \\{"event_id":"evt_001","type":"email.received","data":{"from":"a@b.com"}}
    ;
    const parsed = try std.json.parseFromSlice(WebhookPayload, alloc, body, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    try std.testing.expectEqualStrings("evt_001", parsed.value.event_id);
    try std.testing.expectEqualStrings("email.received", parsed.value.@"type");
}

test "WebhookPayload rejects missing event_id" {
    const alloc = std.testing.allocator;
    const body = \\{"type":"email.received","data":{}}
    ;
    const result = std.json.parseFromSlice(WebhookPayload, alloc, body, .{});
    try std.testing.expect(if (result) |_| false else |_| true);
}
