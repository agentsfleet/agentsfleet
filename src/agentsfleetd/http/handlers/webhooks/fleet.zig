// POST /v1/webhooks/{fleet_id}
//
// Auth: per-fleet HMAC signature only (scheme + secret resolved from the
//       workspace credential keyed by the first webhook trigger's `source`).
//       Verified upstream by the `webhook_sig` middleware before this
//       handler runs. No Bearer fallback — every inbound webhook MUST be
//       signed. Multi-webhook-per-fleet URL routing (`{source}` segment)
//       lands in the install + list response slice; until then the URL
//       carries `fleet_id` alone and the SQL pulls the first webhook
//       trigger from the array.
// Idempotency: Redis key "webhook:dedup:{fleet_id}:{event_id}" — claimed
//       atomically (SET NX EX, so concurrent duplicates still single-enqueue)
//       and RELEASED (DEL) on every post-claim failure path, so a transient
//       serialize/enqueue failure never burns the slot: the sender's retry
//       stays deliverable (loss-proof dedup ordering).
// On success: event enqueued to fleet:{fleet_id}:events stream, returns 202.

const std = @import("std");
const clock = @import("common").clock;
const httpz = @import("httpz");
const pg = @import("pg");
const logging = @import("log");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const fleet_config = @import("../../../fleet_runtime/config.zig");
const telemetry_mod = @import("../../../observability/telemetry.zig");
const metrics_counters = @import("../../../observability/metrics_counters.zig");
const EventEnvelope = @import("contract").event_envelope;
const webhook_parse = @import("webhook_parse.zig");

const S_FLEET_ID_MUST_BE_UUIDV7 = "fleet_id must be a valid UUIDv7";

const log = logging.scoped(.http_webhook);

pub const Context = common.Context;
const Hx = hx_mod.Hx;
const WebhookPayload = webhook_parse.WebhookPayload;

const FleetRow = struct {
    workspace_id: []const u8,
    status: []const u8,
    source: ?[]const u8,
};

fn deinitFleetRow(row: *const FleetRow, alloc: std.mem.Allocator) void {
    alloc.free(row.workspace_id);
    alloc.free(row.status);
    if (row.source) |s| alloc.free(s);
}

fn fetchFleetById(pool: *pg.Pool, alloc: std.mem.Allocator, fleet_id: []const u8) !?FleetRow {
    const conn = try pool.acquire();
    defer pool.release(conn);
    var q = PgQuery.from(try conn.query(
        \\SELECT z.workspace_id::text, z.status,
        \\       (SELECT trig->>'source'
        \\          FROM jsonb_array_elements(z.config_json->'x-agentsfleet'->'triggers') trig
        \\          WHERE trig->>'type' = 'webhook'
        \\          LIMIT 1)
        \\FROM core.fleets z WHERE z.id = $1::uuid
    , .{fleet_id}));
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

fn dedupAndEnqueue(hx: Hx, fleet_id: []const u8, workspace_id: []const u8, payload: WebhookPayload, source_label: []const u8) bool {
    var dedup_key_buf: [256]u8 = undefined;
    const dedup_key = std.fmt.bufPrint(&dedup_key_buf, "{s}{s}:{s}", .{ ec.WEBHOOK_DEDUP_KEY_PREFIX, fleet_id, payload.event_id }) catch {
        common.internalOperationError(hx.res, "Failed to build the duplicate-event check", hx.req_id);
        return false;
    };
    const is_new = hx.ctx.queue.setNx(dedup_key, "1", ec.DEDUP_TTL_SECONDS) catch |err| {
        log.err("redis_dedup_error", .{
            .error_code = ec.ERR_INTERNAL_OPERATION_FAILED,
            .fleet_id = fleet_id,
            .event_id = payload.event_id,
            .err = @errorName(err),
        });
        common.internalOperationError(hx.res, "Failed to check for a duplicate event", hx.req_id);
        return false;
    };
    if (!is_new) {
        log.debug("duplicate", .{ .fleet_id = fleet_id, .event_id = payload.event_id });
        hx.ok(.ok, .{ .status = ec.STATUS_DUPLICATE });
        return false;
    }
    const data_json = std.fmt.allocPrint(hx.alloc, "{f}", .{std.json.fmt(payload.data, .{})}) catch {
        releaseDedupSlot(hx, fleet_id, dedup_key);
        common.internalOperationError(hx.res, "Failed to build the event data", hx.req_id);
        return false;
    };
    defer hx.alloc.free(data_json);

    var actor_buf: [128]u8 = undefined;
    const actor = std.fmt.bufPrint(&actor_buf, "webhook:{s}", .{source_label}) catch "webhook:unknown";
    const envelope = EventEnvelope{
        .event_id = "",
        .fleet_id = fleet_id,
        .workspace_id = workspace_id,
        .actor = actor,
        .event_type = .webhook,
        .request_json = data_json,
        .created_at = clock.nowMillis(),
    };
    const new_event_id = hx.ctx.queue.xaddFleetEvent(envelope) catch |err| {
        // Release the slot so the sender's retry of this delivery stays
        // deliverable (loss-proof dedup ordering).
        releaseDedupSlot(hx, fleet_id, dedup_key);
        log.err("enqueue_failed", .{
            .error_code = ec.ERR_INTERNAL_OPERATION_FAILED,
            .fleet_id = fleet_id,
            .sender_event_id = payload.event_id,
            .err = @errorName(err),
        });
        common.internalOperationError(hx.res, "Failed to enqueue event", hx.req_id);
        return false;
    };
    defer hx.ctx.alloc.free(new_event_id);
    log.debug("enqueued", .{
        .fleet_id = fleet_id,
        .stream_event_id = new_event_id,
        .sender_event_id = payload.event_id,
        .actor = actor,
    });
    return true;
}

/// Release a claimed idempotency slot after a post-claim failure so the
/// sender's retry is not answered "duplicate" for an event that never landed.
/// Best-effort: on a DEL failure the slot expires at its TTL (logged).
fn releaseDedupSlot(hx: Hx, fleet_id: []const u8, dedup_key: []const u8) void {
    hx.ctx.queue.del(dedup_key) catch |err| {
        log.warn("dedup_release_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = fleet_id, .err = @errorName(err) });
    };
}

// ── Main handler ───────────────────────────────────────────────────────────

pub fn innerReceiveWebhook(hx: Hx, req: *httpz.Request, fleet_id: []const u8) void {
    // The fleet id lands in a case-sensitive Redis dedup key below while
    // `WHERE id = $1::uuid` folds case in Postgres. Without this guard the same
    // delivery under two spellings resolves to one fleet but two dedup slots,
    // and gets processed twice. Validate before the id is used as a key.
    if (!id_format.isUuidV7(fleet_id)) {
        hx.fail(ec.ERR_UUIDV7_INVALID_ID_SHAPE, S_FLEET_ID_MUST_BE_UUIDV7);
        return;
    }
    const payload = webhook_parse.parseBody(hx, req, fleet_id) orelse return;
    deliverToFleet(hx, fleet_id, payload);
}

/// Svix-signed delivery (Clerk identity events, etc.) routed to a customer
/// fleet's event log. A Svix envelope carries no top-level `event_id` — the
/// `svix-id` header IS the idempotency key — and the whole body is forwarded
/// as the fleet event's data. Signature is verified by the svix middleware
/// before this handler runs.
pub fn innerReceiveSvixWebhook(hx: Hx, req: *httpz.Request, fleet_id: []const u8) void {
    // The fleet id lands in a case-sensitive Redis dedup key below while
    // `WHERE id = $1::uuid` folds case in Postgres. Without this guard the same
    // delivery under two spellings resolves to one fleet but two dedup slots,
    // and gets processed twice. Validate before the id is used as a key.
    if (!id_format.isUuidV7(fleet_id)) {
        hx.fail(ec.ERR_UUIDV7_INVALID_ID_SHAPE, S_FLEET_ID_MUST_BE_UUIDV7);
        return;
    }
    const payload = webhook_parse.parseSvixBody(hx, req, fleet_id) orelse return;
    deliverToFleet(hx, fleet_id, payload);
}

/// Shared post-parse path for both webhook ingress shapes: resolve the fleet,
/// short-circuit paused fleets to 200-ignored, then dedup-and-enqueue.
fn deliverToFleet(hx: Hx, fleet_id: []const u8, payload: WebhookPayload) void {
    var fleet = fetchFleetById(hx.ctx.pool, hx.alloc, fleet_id) catch |err| {
        log.err("db_error", .{
            .error_code = ec.ERR_INTERNAL_DB_QUERY,
            .fleet_id = fleet_id,
            .err = @errorName(err),
            .req_id = hx.req_id,
        });
        common.internalDbError(hx.res, hx.req_id);
        return;
    } orelse {
        log.warn("not_found", .{
            .error_code = ec.ERR_WEBHOOK_NO_AGENT,
            .fleet_id = fleet_id,
            .req_id = hx.req_id,
        });
        hx.fail(ec.ERR_WEBHOOK_NO_AGENT, ec.MSG_AGENTSFLEET_NOT_FOUND);
        return;
    };
    defer deinitFleetRow(&fleet, hx.alloc);

    // Auth is handled by webhook_sig middleware before this handler runs.

    // Paused fleet → 200-ignored, not 4xx: sender retry queues add no value
    // for an intentionally paused fleet, and the dedup slot is NOT consumed
    // so an operator redelivery after resume processes correctly.
    // The triggered metric is not incremented — nothing was accepted.
    const status = fleet_config.FleetStatus.fromSlice(fleet.status) orelse .stopped;
    if (!status.isRunnable()) {
        log.debug("fleet_not_active", .{
            .fleet_id = fleet_id,
            .status = fleet.status,
            .event_id = payload.event_id,
            .req_id = hx.req_id,
        });
        hx.ok(.ok, .{ .ignored = ec.IGNORED_REASON_AGENTSFLEET_PAUSED });
        return;
    }

    const source_label = fleet.source orelse "";
    if (!dedupAndEnqueue(hx, fleet_id, fleet.workspace_id, payload, source_label)) return;

    recordWebhookAccepted(hx.ctx.telemetry, fleet.workspace_id, fleet_id, payload.event_id, source_label);

    log.debug("accepted", .{
        .fleet_id = fleet_id,
        .event_id = payload.event_id,
        .type = payload.type,
    });
    hx.ok(.accepted, .{ .status = ec.STATUS_ACCEPTED, .event_id = payload.event_id });
}

/// Record observability for a successfully accepted webhook (202 path).
/// Increments the Prometheus triggered counter and emits a PostHog event.
/// Webhook path has no authenticated user — distinct_id is workspace_id so
/// fleet events group under the owning workspace in funnels/retention.
fn recordWebhookAccepted(
    tel: *telemetry_mod.Telemetry,
    workspace_id: []const u8,
    fleet_id: []const u8,
    event_id: []const u8,
    source: []const u8,
) void {
    metrics_counters.incFleetsTriggered();
    tel.capture(telemetry_mod.FleetTriggered, .{
        .distinct_id = workspace_id,
        .workspace_id = workspace_id,
        .fleet_id = fleet_id,
        .event_id = event_id,
        .source = source,
    });
}

// Successful 202 path increments fleets_triggered_total and emits the
// fleet_triggered PostHog event.
test "successful webhook acceptance increments fleets_triggered counter" {
    const metrics_fleet = @import("../../../observability/metrics_fleet.zig");
    const tel_mod = @import("../../../observability/telemetry.zig");
    metrics_fleet.resetForTest();
    defer metrics_fleet.resetForTest();
    var tel = tel_mod.Telemetry.initTest();
    const before = metrics_fleet.snapshotFleetFields().fleet_triggered_total;
    recordWebhookAccepted(&tel, "ws_001", "z_001", "evt_001", "webhook");
    try std.testing.expectEqual(@as(u64, 1), metrics_fleet.snapshotFleetFields().fleet_triggered_total - before);
    try tel_mod.TestBackend.assertLastEventIs(.fleet_triggered);
}

// Body-parsing unit tests live with the parser they exercise.
test {
    _ = webhook_parse;
}
