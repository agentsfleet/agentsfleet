// POST /v1/slack/events — Slack Events API receiver.
//
// Routing:
//   1. Verify x-slack-signature (HMAC-SHA256, RULE CTM) → UZ-WH-010 on fail
//   2. Verify x-slack-request-timestamp freshness → UZ-WH-011 on stale
//   3. type=url_verification → echo challenge (HTTP 200)
//   4. event.bot_id present → ignore (bot loop prevention, HTTP 200)
//   5. Lookup workspace_id via workspace_integrations(provider="slack", external_id=team_id)
//   6. No row → HTTP 200 (workspace hasn't installed UseZombie)
//   7. Find Zombie in workspace with Slack event trigger
//   8. Enqueue event to Redis worker queue
//   9. HTTP 200 always (Slack retries on non-200)
//
// Does NOT use hx.authenticated() — signed by Slack signing secret.
// Env var: SLACK_SIGNING_SECRET

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const webhook_verify = @import("../../zombie/webhook_verify.zig");
const workspace_integrations = @import("../../state/workspace_integrations.zig");
const common = @import("common.zig");
const ec = @import("../../errors/error_registry.zig");

const log = std.log.scoped(.http_slack_events);

pub const Context = common.Context;

const SlackEventPayload = struct {
    type: []const u8,
    team_id: ?[]const u8 = null,
    event_id: ?[]const u8 = null,
    challenge: ?[]const u8 = null,
    event: ?SlackEvent = null,
};

const SlackEvent = struct {
    type: []const u8,
    bot_id: ?[]const u8 = null,
};

// ── Handler ───────────────────────────────────────────────────────────────────

pub fn handleSlackEvent(ctx: *Context, req: *httpz.Request, res: *httpz.Response) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const body = req.body() orelse {
        res.status = 200;
        res.body = "{}";
        return;
    };

    if (!verifySlackSignature(req, body, res, req_id)) return;

    const parsed = std.json.parseFromSlice(SlackEventPayload, alloc, body, .{ .ignore_unknown_fields = true }) catch {
        log.warn("slack.events.parse_fail req_id={s}", .{req_id});
        res.status = 200;
        res.body = "{}";
        return;
    };
    defer parsed.deinit();
    const payload = parsed.value;

    // url_verification: echo challenge (Slack setup handshake)
    if (std.mem.eql(u8, payload.type, "url_verification")) {
        const challenge = payload.challenge orelse "";
        log.info("slack.events.url_verification req_id={s}", .{req_id});
        res.status = 200;
        const resp_body = std.fmt.allocPrint(alloc, "{{\"challenge\":\"{s}\"}}", .{challenge}) catch {
            res.body = "{}";
            return;
        };
        res.body = resp_body;
        return;
    }

    if (!std.mem.eql(u8, payload.type, "event_callback")) {
        res.status = 200;
        res.body = "{}";
        return;
    }

    // Bot loop prevention (§4.0)
    if (payload.event) |evt| {
        if (evt.bot_id != null) {
            log.debug("slack.events.bot_loop_skip req_id={s}", .{req_id});
            res.status = 200;
            res.body = "{}";
            return;
        }
    }

    const team_id = payload.team_id orelse {
        res.status = 200;
        res.body = "{}";
        return;
    };

    const conn = ctx.pool.acquire() catch {
        log.err("slack.events.db_acquire_fail req_id={s}", .{req_id});
        // Always 200 to Slack — Slack retries on non-200
        res.status = 200;
        res.body = "{}";
        return;
    };
    defer ctx.pool.release(conn);

    // Lookup workspace via routing record
    const workspace_id = workspace_integrations.lookupWorkspace(conn, alloc, "slack", team_id) catch {
        log.err("slack.events.lookup_fail team_id={s} req_id={s}", .{ team_id, req_id });
        res.status = 200;
        res.body = "{}";
        return;
    } orelse {
        log.debug("slack.events.no_workspace team_id={s} req_id={s}", .{ team_id, req_id });
        res.status = 200;
        res.body = "{}";
        return;
    };

    // Find a Zombie in this workspace configured for Slack events
    const zombie_id = lookupSlackZombie(conn, alloc, workspace_id) catch {
        log.err("slack.events.zombie_lookup_fail workspace_id={s} req_id={s}", .{ workspace_id, req_id });
        res.status = 200;
        res.body = "{}";
        return;
    } orelse {
        log.debug("slack.events.no_zombie workspace_id={s} req_id={s}", .{ workspace_id, req_id });
        res.status = 200;
        res.body = "{}";
        return;
    };

    const event_id = payload.event_id orelse "slack_evt";
    const event_type = if (payload.event) |evt| evt.type else "unknown";

    ctx.queue.xaddZombieEvent(zombie_id, event_id, event_type, "slack", body) catch |err| {
        log.err("slack.events.enqueue_fail zombie_id={s} err={s} req_id={s}", .{ zombie_id, @errorName(err), req_id });
        res.status = 200;
        res.body = "{}";
        return;
    };

    log.info("slack.events.enqueued zombie_id={s} event_id={s} type={s} req_id={s}", .{
        zombie_id, event_id, event_type, req_id,
    });
    res.status = 200;
    res.body = "{}";
}

// ── Internal ─────────────────────────────────────────────────────────────────

/// Verify Slack request signature and timestamp freshness.
/// Returns false and writes UZ-WH-010/011 on failure.
fn verifySlackSignature(req: *httpz.Request, body: []const u8, res: *httpz.Response, req_id: []const u8) bool {
    const secret = std.process.getEnvVarOwned(std.heap.page_allocator, "SLACK_SIGNING_SECRET") catch {
        log.err("slack.events.no_signing_secret req_id={s}", .{req_id});
        common.errorResponse(res, ec.ERR_WEBHOOK_SLACK_SIG_INVALID, "Signing secret not configured", req_id);
        return false;
    };
    defer std.heap.page_allocator.free(secret);

    const timestamp = req.header(ec.SLACK_TS_HEADER) orelse {
        common.errorResponse(res, ec.ERR_WEBHOOK_SLACK_SIG_INVALID, "Missing timestamp header", req_id);
        return false;
    };
    const signature = req.header(ec.SLACK_SIG_HEADER) orelse {
        common.errorResponse(res, ec.ERR_WEBHOOK_SLACK_SIG_INVALID, "Missing signature header", req_id);
        return false;
    };

    if (!webhook_verify.isTimestampFresh(timestamp, ec.SLACK_MAX_TS_DRIFT_SECONDS)) {
        common.errorResponse(res, ec.ERR_WEBHOOK_SLACK_TIMESTAMP_STALE, "Timestamp too old", req_id);
        return false;
    }

    if (!webhook_verify.verifySignature(webhook_verify.SLACK, secret, timestamp, body, signature)) {
        log.warn("slack.events.sig_invalid req_id={s}", .{req_id});
        common.errorResponse(res, ec.ERR_WEBHOOK_SLACK_SIG_INVALID, "Invalid signature", req_id);
        return false;
    }

    return true;
}

/// Find the first active Zombie in `workspace_id` with a Slack event trigger.
/// Returns owned zombie_id or null if none configured.
fn lookupSlackZombie(conn: *pg.Conn, alloc: std.mem.Allocator, workspace_id: []const u8) !?[]const u8 {
    var q = PgQuery.from(conn.query(
        \\SELECT id::text FROM core.zombies
        \\WHERE workspace_id = $1::uuid
        \\  AND status = 'active'
        \\  AND config_json->'trigger'->>'type' = 'slack_event'
        \\LIMIT 1
    , .{workspace_id}) catch |err| {
        log.err("slack.events.zombie_query_fail workspace_id={s} err={s}", .{ workspace_id, @errorName(err) });
        return err;
    });
    defer q.deinit();

    const row = try q.next() orelse return null;
    const raw = try row.get([]u8, 0);
    return try alloc.dupe(u8, raw);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

test "url_verification payload parsed" {
    const alloc = std.testing.allocator;
    const body =
        \\{"type":"url_verification","challenge":"abc123","token":"xxx"}
    ;
    const parsed = try std.json.parseFromSlice(SlackEventPayload, alloc, body, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    try std.testing.expectEqualStrings("url_verification", parsed.value.type);
    try std.testing.expectEqualStrings("abc123", parsed.value.challenge.?);
}

test "event_callback with bot_id is bot event" {
    const alloc = std.testing.allocator;
    const body =
        \\{"type":"event_callback","team_id":"T01","event_id":"Ev01","event":{"type":"message","bot_id":"B01"}}
    ;
    const parsed = try std.json.parseFromSlice(SlackEventPayload, alloc, body, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    try std.testing.expect(parsed.value.event.?.bot_id != null);
}

test "event_callback without bot_id is human event" {
    const alloc = std.testing.allocator;
    const body =
        \\{"type":"event_callback","team_id":"T01","event_id":"Ev02","event":{"type":"message"}}
    ;
    const parsed = try std.json.parseFromSlice(SlackEventPayload, alloc, body, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    try std.testing.expect(parsed.value.event.?.bot_id == null);
}
