// POST /v1/slack/events — Slack Events API receiver.
//
// Routing:
//   1. Verify x-slack-signature (HMAC-SHA256, RULE CTM) → UZ-WH-010 on fail
//   2. Verify x-slack-request-timestamp freshness → UZ-WH-011 on stale
//   3. type=url_verification → echo challenge (HTTP 200)
//   4. event.bot_id present or subtype=bot_message → ignore (bot loop prevention, HTTP 200)
//   5. Lookup workspace_id via workspace_integrations(provider="slack", external_id=team_id)
//   6. No row → HTTP 200 (workspace hasn't installed UseZombie)
//   7. Find Zombie in workspace with Slack event trigger
//   8. Enqueue event to Redis worker queue
//   9. HTTP 200 on success; HTTP 503 on infra failure (pool/DB/Redis) so Slack retries
//
// Does NOT use hx.authenticated() — signed by Slack signing secret.
// Env var: SLACK_SIGNING_SECRET

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const workspace_integrations = @import("../../state/workspace_integrations.zig");
const common = @import("common.zig");
const hx_mod = @import("hx.zig");
const ec = @import("../../errors/error_registry.zig");

const log = std.log.scoped(.http_slack_events);

pub const Context = common.Context;
const Hx = hx_mod.Hx;

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
    subtype: ?[]const u8 = null,
};

// ── Handler ───────────────────────────────────────────────────────────────────

pub fn innerSlackEvent(hx: Hx, req: *httpz.Request) void {
    const body = req.body() orelse {
        hx.res.status = 200;
        hx.res.body = "{}";
        return;
    };

    // Slack signature verified by slack_signature middleware before this handler runs.

    const parsed = std.json.parseFromSlice(SlackEventPayload, hx.alloc, body, .{ .ignore_unknown_fields = true }) catch {
        log.warn("slack.events.parse_fail req_id={s}", .{hx.req_id});
        hx.res.status = 200;
        hx.res.body = "{}";
        return;
    };
    defer parsed.deinit();
    const payload = parsed.value;

    // url_verification: echo challenge (Slack setup handshake)
    if (std.mem.eql(u8, payload.type, "url_verification")) {
        const challenge = payload.challenge orelse "";
        log.info("slack.events.url_verification req_id={s}", .{hx.req_id});
        hx.ok(.ok, .{ .challenge = challenge });
        return;
    }

    if (!std.mem.eql(u8, payload.type, "event_callback")) {
        hx.res.status = 200;
        hx.res.body = "{}";
        return;
    }

    // Bot loop prevention (§4.0): skip bot_id events and bot_message subtypes
    if (payload.event) |evt| {
        const is_bot = evt.bot_id != null or
            (if (evt.subtype) |st| std.mem.eql(u8, st, "bot_message") else false);
        if (is_bot) {
            log.debug("slack.events.bot_loop_skip req_id={s}", .{hx.req_id});
            hx.res.status = 200;
            hx.res.body = "{}";
            return;
        }
    }

    const team_id = payload.team_id orelse {
        hx.res.status = 200;
        hx.res.body = "{}";
        return;
    };

    const conn = hx.ctx.pool.acquire() catch {
        log.err("slack.events.db_acquire_fail req_id={s}", .{hx.req_id});
        // 503 so Slack retries — returning 200 would permanently lose the event
        hx.res.status = 503;
        hx.res.body = "{}";
        return;
    };
    defer hx.ctx.pool.release(conn);

    // Lookup workspace via routing record
    const workspace_id = workspace_integrations.lookupWorkspace(conn, hx.alloc, "slack", team_id) catch {
        log.err("slack.events.lookup_fail team_id={s} req_id={s}", .{ team_id, hx.req_id });
        hx.res.status = 503; // DB error — Slack should retry
        hx.res.body = "{}";
        return;
    } orelse {
        log.debug("slack.events.no_workspace team_id={s} req_id={s}", .{ team_id, hx.req_id });
        hx.res.status = 200; // workspace not installed — not an error, no retry needed
        hx.res.body = "{}";
        return;
    };

    // Find a Zombie in this workspace configured for Slack events
    const zombie_id = lookupSlackZombie(conn, hx.alloc, workspace_id) catch {
        log.err("slack.events.zombie_lookup_fail workspace_id={s} req_id={s}", .{ workspace_id, hx.req_id });
        hx.res.status = 503; // DB error — Slack should retry
        hx.res.body = "{}";
        return;
    } orelse {
        log.debug("slack.events.no_zombie workspace_id={s} req_id={s}", .{ workspace_id, hx.req_id });
        hx.res.status = 200; // no Slack-trigger zombie configured — not an error
        hx.res.body = "{}";
        return;
    };

    const event_id = payload.event_id orelse "slack_evt";
    const event_type = if (payload.event) |evt| evt.type else "unknown";

    hx.ctx.queue.xaddZombieEvent(zombie_id, event_id, event_type, "slack", body) catch |err| {
        log.err("slack.events.enqueue_fail zombie_id={s} err={s} req_id={s}", .{ zombie_id, @errorName(err), hx.req_id });
        hx.res.status = 503; // Redis error — Slack should retry
        hx.res.body = "{}";
        return;
    };

    log.info("slack.events.enqueued zombie_id={s} event_id={s} type={s} req_id={s}", .{
        zombie_id, event_id, event_type, hx.req_id,
    });
    hx.res.status = 200;
    hx.res.body = "{}";
}

// ── Internal ─────────────────────────────────────────────────────────────────

/// Find the first active Zombie in `workspace_id` with a Slack event trigger.
/// Returns owned zombie_id or null if none configured.
fn lookupSlackZombie(conn: *pg.Conn, alloc: std.mem.Allocator, workspace_id: []const u8) !?[]const u8 {
    var q = PgQuery.from(conn.query(
        \\SELECT id::text FROM core.zombies
        \\WHERE workspace_id = $1::uuid
        \\  AND status = 'active'
        \\  AND config_json->'trigger'->>'type' = 'slack_event'
        \\ORDER BY created_at ASC
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

test "event_callback with subtype=bot_message is bot event (bot loop §4.0)" {
    const alloc = std.testing.allocator;
    const body =
        \\{"type":"event_callback","team_id":"T01","event_id":"Ev03","event":{"type":"message","subtype":"bot_message","text":"hello"}}
    ;
    const parsed = try std.json.parseFromSlice(SlackEventPayload, alloc, body, .{ .ignore_unknown_fields = true });
    defer parsed.deinit();
    const evt = parsed.value.event.?;
    try std.testing.expect(evt.bot_id == null); // no bot_id on subtype path
    try std.testing.expectEqualStrings("bot_message", evt.subtype.?);
    // Verify the combined check: bot_id == null but subtype triggers is_bot
    const is_bot = evt.bot_id != null or
        (if (evt.subtype) |st| std.mem.eql(u8, st, "bot_message") else false);
    try std.testing.expect(is_bot);
}
