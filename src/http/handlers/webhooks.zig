// POST /v1/webhooks/{zombie_id} — inbound event delivery for Zombies.
//
// Routing: zombie_id in the URL is the primary key of core.zombies.
//   No JSONB traversal, no unique-source constraint needed.
//   The trigger.source field in config is a human label (for logs), not a routing key.
//
// Auth: Bearer token matched against core.zombies.config_json->'trigger'->>'token'.
//   If no token is configured the endpoint is open (dev/test mode).
//
// Idempotency: Redis SET NX EX 86400 on "webhook:dedup:{zombie_id}:{event_id}".
// On success: event enqueued to zombie:{zombie_id}:events stream, returns 202.
//
// Error codes:
//   UZ-WH-001  zombie_id not found or not in this workspace (404)
//   UZ-WH-002  Malformed or missing event payload (400)
//   UZ-WH-003  Zombie is paused or stopped (409)

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const common = @import("common.zig");
const error_codes = @import("../../errors/codes.zig");
const activity_stream = @import("../../zombie/activity_stream.zig");

const log = std.log.scoped(.http_webhook);

pub const Context = common.Context;

// Inbound webhook payload shape: {"event_id": "...", "type": "...", "data": {...}}
const WebhookPayload = struct {
    event_id: []const u8,
    @"type": []const u8,
    data: std.json.Value,
};

// Columns fetched from core.zombies by primary key.
const ZombieRow = struct {
    workspace_id: []u8,
    status: []u8,
    token: ?[]u8,   // config_json->'trigger'->>'token' — null means open (no auth)
    source: ?[]u8,  // config_json->'trigger'->>'source' — label for logs/stream, not routing key
};

fn deinitZombieRow(row: *ZombieRow, alloc: std.mem.Allocator) void {
    alloc.free(row.workspace_id);
    alloc.free(row.status);
    if (row.token) |t| alloc.free(t);
    if (row.source) |s| alloc.free(s);
}

// fetchZombieById looks up core.zombies by primary key.
// PK lookup — O(1), no JSONB index required.
// Returns null when zombie_id is not found.
fn fetchZombieById(
    pool: *pg.Pool,
    alloc: std.mem.Allocator,
    zombie_id: []const u8,
) !?ZombieRow {
    const conn = try pool.acquire();
    defer pool.release(conn);

    var q = try conn.query( // check-pg-drain: ok — drain called below
        \\SELECT workspace_id, status,
        \\       config_json->'trigger'->>'token',
        \\       config_json->'trigger'->>'source'
        \\FROM core.zombies
        \\WHERE id = $1::uuid
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
    return ZombieRow{
        .workspace_id = workspace_id,
        .status = status,
        .token = token,
        .source = source,
    };
}

// ── Handler ─────────────────────────────────────────────────────────────────

pub fn handleReceiveWebhook(
    ctx: *Context,
    req: *httpz.Request,
    res: *httpz.Response,
    zombie_id: []const u8,
) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    // ── 1. Parse body ──────────────────────────────────────────────────────
    const body = req.body() orelse {
        log.warn("webhook.no_body zombie_id={s} req_id={s}", .{ zombie_id, req_id });
        common.errorResponse(res, .bad_request, error_codes.ERR_WEBHOOK_MALFORMED, "Request body required", req_id);
        return;
    };
    if (!common.checkBodySize(req, res, body, req_id)) return;

    const parsed = std.json.parseFromSlice(WebhookPayload, alloc, body, .{ .ignore_unknown_fields = true }) catch {
        log.warn("webhook.malformed_json zombie_id={s} req_id={s}", .{ zombie_id, req_id });
        common.errorResponse(res, .bad_request, error_codes.ERR_WEBHOOK_MALFORMED, "Malformed JSON", req_id);
        return;
    };
    defer parsed.deinit();
    const payload = parsed.value;

    if (payload.event_id.len == 0 or payload.@"type".len == 0) {
        common.errorResponse(res, .bad_request, error_codes.ERR_WEBHOOK_MALFORMED, "event_id and type are required", req_id);
        return;
    }

    // ── 2. Fetch zombie by primary key ─────────────────────────────────────
    var zombie = fetchZombieById(ctx.pool, alloc, zombie_id) catch |err| {
        log.err("webhook.db_error zombie_id={s} err={s} req_id={s}", .{ zombie_id, @errorName(err), req_id });
        common.internalDbError(res, req_id);
        return;
    } orelse {
        log.warn("webhook.not_found zombie_id={s} req_id={s} error_code=" ++ error_codes.ERR_WEBHOOK_NO_ZOMBIE, .{ zombie_id, req_id });
        common.errorResponse(res, .not_found, error_codes.ERR_WEBHOOK_NO_ZOMBIE, "Zombie not found", req_id);
        return;
    };
    defer deinitZombieRow(&zombie, alloc);

    // ── 3. Auth — Bearer token against zombie's trigger.token ──────────────
    if (zombie.token) |expected_token| {
        const auth_header = req.header("authorization") orelse {
            common.errorResponse(res, .unauthorized, "UZ-AUTH-001", "Authorization required", req_id);
            return;
        };
        const bearer_prefix = "Bearer ";
        if (!std.mem.startsWith(u8, auth_header, bearer_prefix)) {
            common.errorResponse(res, .unauthorized, "UZ-AUTH-001", "Bearer token required", req_id);
            return;
        }
        const provided = auth_header[bearer_prefix.len..];
        // XOR accumulation: constant-time comparison to prevent timing attacks.
        // Length inequality is safe to short-circuit — length is not a secret.
        const token_valid = provided.len == expected_token.len and ct: {
            var diff: u8 = 0;
            for (provided, expected_token) |a, b| diff |= a ^ b;
            break :ct diff == 0;
        };
        if (!token_valid) {
            common.errorResponse(res, .unauthorized, "UZ-AUTH-001", "Invalid token", req_id);
            return;
        }
    }
    // No token configured — open endpoint (dev/test mode).

    // ── 4. Check zombie status ─────────────────────────────────────────────
    if (!std.mem.eql(u8, zombie.status, "active")) {
        log.warn("webhook.zombie_not_active zombie_id={s} status={s} req_id={s} error_code=" ++ error_codes.ERR_WEBHOOK_ZOMBIE_PAUSED, .{ zombie_id, zombie.status, req_id });
        common.errorResponse(res, .conflict, error_codes.ERR_WEBHOOK_ZOMBIE_PAUSED, "Zombie is not active", req_id);
        return;
    }

    // ── 5. Idempotency dedup — Redis SET NX EX 86400 ──────────────────────
    var dedup_key_buf: [256]u8 = undefined;
    const dedup_key = std.fmt.bufPrint(&dedup_key_buf, "webhook:dedup:{s}:{s}", .{ zombie_id, payload.event_id }) catch {
        common.internalOperationError(res, "dedup key overflow", req_id);
        return;
    };
    const is_new = ctx.queue.setNx(dedup_key, "1", 86400) catch |err| {
        log.err("webhook.redis_dedup_error zombie_id={s} event_id={s} err={s}", .{ zombie_id, payload.event_id, @errorName(err) });
        common.internalOperationError(res, "Idempotency check failed", req_id);
        return;
    };
    if (!is_new) {
        log.debug("webhook.duplicate zombie_id={s} event_id={s}", .{ zombie_id, payload.event_id });
        res.status = 200;
        res.json(.{ .status = "duplicate" }, .{}) catch {};
        return;
    }

    // ── 6. Serialize data field for stream entry ───────────────────────────
    const data_json = std.fmt.allocPrint(alloc, "{f}", .{std.json.fmt(payload.data, .{})}) catch {
        common.internalOperationError(res, "Failed to serialize event data", req_id);
        return;
    };

    // ── 7. Enqueue to zombie event stream ──────────────────────────────────
    // source label is from config (human-readable, e.g. "email") — falls back to empty.
    const source_label = zombie.source orelse "";
    ctx.queue.xaddZombieEvent(zombie_id, payload.event_id, payload.@"type", source_label, data_json) catch |err| {
        log.err("webhook.enqueue_failed zombie_id={s} event_id={s} err={s}", .{ zombie_id, payload.event_id, @errorName(err) });
        common.internalOperationError(res, "Failed to enqueue event", req_id);
        return;
    };

    // ── 8. Activity log (fire-and-forget) ─────────────────────────────────
    var detail_buf: [256]u8 = undefined;
    const detail = std.fmt.bufPrint(
        &detail_buf,
        "webhook_received source={s} event_id={s} type={s}",
        .{ source_label, payload.event_id, payload.@"type" },
    ) catch "webhook_received";
    activity_stream.logEvent(ctx.pool, alloc, .{
        .zombie_id = zombie_id,
        .workspace_id = zombie.workspace_id,
        .event_type = "webhook_received",
        .detail = detail,
    });

    log.info("webhook.accepted zombie_id={s} event_id={s} type={s} source={s}", .{
        zombie_id, payload.event_id, payload.@"type", source_label,
    });

    res.status = 202;
    res.json(.{ .status = "accepted", .event_id = payload.event_id }, .{}) catch {};
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
