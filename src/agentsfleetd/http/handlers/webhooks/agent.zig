// POST /v1/webhooks/{agent_id}
//
// Auth: per-agent HMAC signature only (scheme + secret resolved from the
//       workspace credential keyed by the first webhook trigger's `source`).
//       Verified upstream by the `webhook_sig` middleware before this
//       handler runs. No Bearer fallback — every inbound webhook MUST be
//       signed. Multi-webhook-per-agent URL routing (`{source}` segment)
//       lands in the install + list response slice; until then the URL
//       carries `agent_id` alone and the SQL pulls the first webhook
//       trigger from the array.
// Idempotency: Redis key "webhook:dedup:{agent_id}:{event_id}" — claimed
//       atomically (SET NX EX, so concurrent duplicates still single-enqueue)
//       and RELEASED (DEL) on every post-claim failure path, so a transient
//       serialize/enqueue failure never burns the slot: the sender's retry
//       stays deliverable (loss-proof dedup ordering).
// On success: event enqueued to agent:{agent_id}:events stream, returns 202.

const std = @import("std");
const clock = @import("common").clock;
const httpz = @import("httpz");
const pg = @import("pg");
const logging = @import("log");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const agent_config = @import("../../../agent/config.zig");
const telemetry_mod = @import("../../../observability/telemetry.zig");
const metrics_counters = @import("../../../observability/metrics_counters.zig");
const EventEnvelope = @import("contract").event_envelope;

const log = logging.scoped(.http_webhook);

pub const Context = common.Context;
const Hx = hx_mod.Hx;

const WebhookPayload = struct {
    event_id: []const u8,
    type: []const u8,
    data: std.json.Value,
};

const AgentRow = struct {
    workspace_id: []const u8,
    status: []const u8,
    source: ?[]const u8,
};

fn deinitAgentRow(row: *const AgentRow, alloc: std.mem.Allocator) void {
    alloc.free(row.workspace_id);
    alloc.free(row.status);
    if (row.source) |s| alloc.free(s);
}

fn fetchAgentById(pool: *pg.Pool, alloc: std.mem.Allocator, agent_id: []const u8) !?AgentRow {
    const conn = try pool.acquire();
    defer pool.release(conn);
    var q = PgQuery.from(try conn.query(
        \\SELECT z.workspace_id::text, z.status,
        \\       (SELECT trig->>'source'
        \\          FROM jsonb_array_elements(z.config_json->'x-agentsfleet'->'triggers') trig
        \\          WHERE trig->>'type' = 'webhook'
        \\          LIMIT 1)
        \\FROM core.agents z WHERE z.id = $1::uuid
    , .{agent_id}));
    defer q.deinit();
    const row = try q.next() orelse return null;
    const workspace_id = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(workspace_id);
    const status = try alloc.dupe(u8, try row.get([]const u8, 1));
    errdefer alloc.free(status);
    const source_raw = row.get([]const u8, 2) catch null;
    const source: ?[]const u8 = if (source_raw) |s| try alloc.dupe(u8, s) else null;
    return .{ .workspace_id = workspace_id, .status = status, .source = source };
}

// ── Helpers ───────────────────────────────────────────────────────────────

fn parseBody(hx: Hx, req: *httpz.Request, agent_id: []const u8) ?WebhookPayload {
    const body = req.body() orelse {
        log.warn("no_body", .{
            .error_code = ec.ERR_WEBHOOK_MALFORMED,
            .agent_id = agent_id,
            .req_id = hx.req_id,
        });
        hx.fail(ec.ERR_WEBHOOK_MALFORMED, ec.MSG_BODY_REQUIRED);
        return null;
    };
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return null;
    const parsed = std.json.parseFromSlice(WebhookPayload, hx.alloc, body, .{ .ignore_unknown_fields = true }) catch {
        log.warn("malformed_json", .{
            .error_code = ec.ERR_WEBHOOK_MALFORMED,
            .agent_id = agent_id,
            .req_id = hx.req_id,
        });
        hx.fail(ec.ERR_WEBHOOK_MALFORMED, ec.MSG_MALFORMED_JSON);
        return null;
    };
    const payload = parsed.value;
    if (payload.event_id.len == 0 or payload.type.len == 0) {
        log.warn("missing_fields", .{
            .error_code = ec.ERR_WEBHOOK_MALFORMED,
            .agent_id = agent_id,
            .req_id = hx.req_id,
        });
        hx.fail(ec.ERR_WEBHOOK_MALFORMED, ec.MSG_MISSING_FIELDS);
        parsed.deinit();
        return null;
    }
    return payload;
}

fn dedupAndEnqueue(hx: Hx, agent_id: []const u8, workspace_id: []const u8, payload: WebhookPayload, source_label: []const u8) bool {
    var dedup_key_buf: [256]u8 = undefined;
    const dedup_key = std.fmt.bufPrint(&dedup_key_buf, "{s}{s}:{s}", .{ ec.WEBHOOK_DEDUP_KEY_PREFIX, agent_id, payload.event_id }) catch {
        common.internalOperationError(hx.res, "dedup key overflow", hx.req_id);
        return false;
    };
    const is_new = hx.ctx.queue.setNx(dedup_key, "1", ec.DEDUP_TTL_SECONDS) catch |err| {
        log.err("redis_dedup_error", .{
            .error_code = ec.ERR_INTERNAL_OPERATION_FAILED,
            .agent_id = agent_id,
            .event_id = payload.event_id,
            .err = @errorName(err),
        });
        common.internalOperationError(hx.res, "Idempotency check failed", hx.req_id);
        return false;
    };
    if (!is_new) {
        log.debug("duplicate", .{ .agent_id = agent_id, .event_id = payload.event_id });
        hx.ok(.ok, .{ .status = ec.STATUS_DUPLICATE });
        return false;
    }
    const data_json = std.fmt.allocPrint(hx.alloc, "{f}", .{std.json.fmt(payload.data, .{})}) catch {
        releaseDedupSlot(hx, agent_id, dedup_key);
        common.internalOperationError(hx.res, "Failed to serialize event data", hx.req_id);
        return false;
    };
    defer hx.alloc.free(data_json);

    var actor_buf: [128]u8 = undefined;
    const actor = std.fmt.bufPrint(&actor_buf, "webhook:{s}", .{source_label}) catch "webhook:unknown";
    const envelope = EventEnvelope{
        .event_id = "",
        .agent_id = agent_id,
        .workspace_id = workspace_id,
        .actor = actor,
        .event_type = .webhook,
        .request_json = data_json,
        .created_at = clock.nowMillis(),
    };
    const new_event_id = hx.ctx.queue.xaddAgentEvent(envelope) catch |err| {
        // Release the slot so the sender's retry of this delivery stays
        // deliverable (loss-proof dedup ordering).
        releaseDedupSlot(hx, agent_id, dedup_key);
        log.err("enqueue_failed", .{
            .error_code = ec.ERR_INTERNAL_OPERATION_FAILED,
            .agent_id = agent_id,
            .sender_event_id = payload.event_id,
            .err = @errorName(err),
        });
        common.internalOperationError(hx.res, "Failed to enqueue event", hx.req_id);
        return false;
    };
    defer hx.ctx.alloc.free(new_event_id);
    log.info("enqueued", .{
        .agent_id = agent_id,
        .stream_event_id = new_event_id,
        .sender_event_id = payload.event_id,
        .actor = actor,
    });
    return true;
}

/// Release a claimed idempotency slot after a post-claim failure so the
/// sender's retry is not answered "duplicate" for an event that never landed.
/// Best-effort: on a DEL failure the slot expires at its TTL (logged).
fn releaseDedupSlot(hx: Hx, agent_id: []const u8, dedup_key: []const u8) void {
    hx.ctx.queue.del(dedup_key) catch |err| {
        log.warn("dedup_release_failed", .{ .agent_id = agent_id, .err = @errorName(err) });
    };
}

// ── Main handler ───────────────────────────────────────────────────────────

pub fn innerReceiveWebhook(hx: Hx, req: *httpz.Request, agent_id: []const u8) void {
    const payload = parseBody(hx, req, agent_id) orelse return;

    var agent = fetchAgentById(hx.ctx.pool, hx.alloc, agent_id) catch |err| {
        log.err("db_error", .{
            .error_code = ec.ERR_INTERNAL_DB_QUERY,
            .agent_id = agent_id,
            .err = @errorName(err),
            .req_id = hx.req_id,
        });
        common.internalDbError(hx.res, hx.req_id);
        return;
    } orelse {
        log.warn("not_found", .{
            .error_code = ec.ERR_WEBHOOK_NO_AGENT,
            .agent_id = agent_id,
            .req_id = hx.req_id,
        });
        hx.fail(ec.ERR_WEBHOOK_NO_AGENT, ec.MSG_AGENTSFLEET_NOT_FOUND);
        return;
    };
    defer deinitAgentRow(&agent, hx.alloc);

    // Auth is handled by webhook_sig middleware before this handler runs.

    // Paused agent → 200-ignored, not 4xx: sender retry queues add no value
    // for an intentionally paused agent, and the dedup slot is NOT consumed
    // so an operator redelivery after resume processes correctly.
    // The triggered metric is not incremented — nothing was accepted.
    const status = agent_config.AgentStatus.fromSlice(agent.status) orelse .stopped;
    if (!status.isRunnable()) {
        log.info("agent_not_active", .{
            .agent_id = agent_id,
            .status = agent.status,
            .event_id = payload.event_id,
            .req_id = hx.req_id,
        });
        hx.ok(.ok, .{ .ignored = ec.IGNORED_REASON_AGENTSFLEET_PAUSED });
        return;
    }

    const source_label = agent.source orelse "";
    if (!dedupAndEnqueue(hx, agent_id, agent.workspace_id, payload, source_label)) return;

    recordWebhookAccepted(hx.ctx.telemetry, agent.workspace_id, agent_id, payload.event_id, source_label);

    log.info("accepted", .{
        .agent_id = agent_id,
        .event_id = payload.event_id,
        .type = payload.type,
    });
    hx.ok(.accepted, .{ .status = ec.STATUS_ACCEPTED, .event_id = payload.event_id });
}

/// Record observability for a successfully accepted webhook (202 path).
/// Increments the Prometheus triggered counter and emits a PostHog event.
/// Webhook path has no authenticated user — distinct_id is workspace_id so
/// agent events group under the owning workspace in funnels/retention.
fn recordWebhookAccepted(
    tel: *telemetry_mod.Telemetry,
    workspace_id: []const u8,
    agent_id: []const u8,
    event_id: []const u8,
    source: []const u8,
) void {
    metrics_counters.incAgentsTriggered();
    tel.capture(telemetry_mod.AgentTriggered, .{
        .distinct_id = workspace_id,
        .workspace_id = workspace_id,
        .agent_id = agent_id,
        .event_id = event_id,
        .source = source,
    });
}

// Successful 202 path increments agents_triggered_total and emits the
// agent_triggered PostHog event.
test "successful webhook acceptance increments agents_triggered counter" {
    const metrics_agent = @import("../../../observability/metrics_agent.zig");
    const tel_mod = @import("../../../observability/telemetry.zig");
    metrics_agent.resetForTest();
    defer metrics_agent.resetForTest();
    var tel = tel_mod.Telemetry.initTest();
    const before = metrics_agent.snapshotAgentFields().agent_triggered_total;
    recordWebhookAccepted(&tel, "ws_001", "z_001", "evt_001", "webhook");
    try std.testing.expectEqual(@as(u64, 1), metrics_agent.snapshotAgentFields().agent_triggered_total - before);
    try tel_mod.TestBackend.assertLastEventIs(.agent_triggered);
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
