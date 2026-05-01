// POST /v1/webhooks/{zombie_id}/github — GitHub Actions webhook ingest.
//
// Auth: HMAC-SHA256 over the raw body (X-Hub-Signature-256), verified by the
//       webhook_sig middleware against the workspace's `zombie:github`
//       credential. This handler runs only after the signature is valid.
//
// Body cap: 1 MiB (UZ-WH-030 before any other work).
// Filter: only `workflow_run` events with `action=completed` and
//         `conclusion=failure` are XADDed; everything else returns 200 OK
//         with a `{"ignored":"<reason>"}` body so the diagnostic survives
//         CDN / HTTP/2 proxy paths (RFC 9110 §6.4.5 forbids 204+body).
// Idempotency: webhook:dedup:{zombie_id}:gh:{X-GitHub-Delivery} EX 86400.
// On accept: normalized envelope is XADDed to zombie:{id}:events with
//            `actor=webhook:github`, `event_type=webhook`. Returns 202.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");

const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const zombie_config = @import("../../../zombie/config.zig");
const telemetry_mod = @import("../../../observability/telemetry.zig");
const metrics_counters = @import("../../../observability/metrics_counters.zig");
const EventEnvelope = @import("../../../zombie/event_envelope.zig");
const normalizer = @import("../../../zombie/webhook/normalizer/github.zig");

const log = std.log.scoped(.http_webhook_github);

const Hx = hx_mod.Hx;

const MAX_BODY_BYTES: usize = 1 * 1024 * 1024;
const ACTOR = "webhook:github";
const PROVIDER_DEDUP_NAMESPACE = "gh";
const HEADER_EVENT = "x-github-event";
const HEADER_DELIVERY = "x-github-delivery";
const EVENT_WORKFLOW_RUN = "workflow_run";
const ACTION_COMPLETED = "completed";
const CONCLUSION_FAILURE = "failure";

const FilterDecision = struct { ingest: bool, reason: []const u8 };

pub fn innerInvokeGithubWebhook(hx: Hx, req: *httpz.Request, zombie_id: []const u8) void {
    // Pre-read fence: reject oversized payloads before httpz buffers them.
    // The httpz server-level max_body_size may be larger than our 1 MiB cap,
    // so without this guard a >1 MiB body would be fully buffered + discarded.
    if (req.header("content-length")) |cl_str| {
        const cl = std.fmt.parseInt(usize, cl_str, 10) catch 0;
        if (cl > MAX_BODY_BYTES) {
            hx.fail(ec.ERR_WEBHOOK_PAYLOAD_TOO_LARGE, "Webhook body exceeds 1 MiB");
            return;
        }
    }
    const body = req.body() orelse {
        hx.fail(ec.ERR_WEBHOOK_MALFORMED, ec.MSG_BODY_REQUIRED);
        return;
    };
    // Post-buffer guard: catches payloads sent without (or with a lying)
    // Content-Length header.
    if (body.len > MAX_BODY_BYTES) {
        hx.fail(ec.ERR_WEBHOOK_PAYLOAD_TOO_LARGE, "Webhook body exceeds 1 MiB");
        return;
    }

    const event = req.header(HEADER_EVENT) orelse {
        hx.fail(ec.ERR_WEBHOOK_MALFORMED, "Missing X-GitHub-Event header");
        return;
    };
    const delivery = req.header(HEADER_DELIVERY) orelse {
        hx.fail(ec.ERR_WEBHOOK_MALFORMED, "Missing X-GitHub-Delivery header");
        return;
    };

    if (!std.mem.eql(u8, event, EVENT_WORKFLOW_RUN)) {
        // 200 OK + diagnostic body (not 204) so the `ignored` reason survives
        // CDNs / HTTP/2 proxies that may strip or reject 204+body per
        // RFC 9110 §6.4.5. GitHub's webhook delivery dashboard renders this
        // body when an operator inspects "Recent Deliveries".
        hx.ok(.ok, .{ .ignored = event });
        return;
    }

    if (!claimDeliveryKey(hx, zombie_id, delivery)) return;

    var zombie = fetchZombieById(hx.ctx.pool, hx.alloc, zombie_id) catch |err| {
        log.err("github_webhook.db_error zombie_id={s} err={s} req_id={s}", .{ zombie_id, @errorName(err), hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    } orelse {
        hx.fail(ec.ERR_WEBHOOK_NO_ZOMBIE, ec.MSG_ZOMBIE_NOT_FOUND);
        return;
    };
    defer deinitZombieRow(&zombie, hx.alloc);

    const status = zombie_config.ZombieStatus.fromSlice(zombie.status) orelse .stopped;
    if (!status.isRunnable()) {
        hx.fail(ec.ERR_WEBHOOK_ZOMBIE_PAUSED, ec.MSG_ZOMBIE_NOT_ACTIVE);
        return;
    }

    const decision = filterAction(hx.alloc, body) orelse {
        hx.fail(ec.ERR_WEBHOOK_MALFORMED, ec.MSG_MALFORMED_JSON);
        return;
    };
    if (!decision.ingest) {
        hx.ok(.ok, .{ .ignored = decision.reason });
        return;
    }

    const request_json = normalizer.normalize(hx.alloc, body, std.time.timestamp()) catch |err| {
        log.err("github_webhook.normalize_failed zombie_id={s} err={s} req_id={s}", .{ zombie_id, @errorName(err), hx.req_id });
        hx.fail(ec.ERR_WEBHOOK_MALFORMED, ec.MSG_MALFORMED_JSON);
        return;
    };
    defer hx.alloc.free(request_json);

    const envelope = EventEnvelope{
        .event_id = "",
        .zombie_id = zombie_id,
        .workspace_id = zombie.workspace_id,
        .actor = ACTOR,
        .event_type = .webhook,
        .request_json = request_json,
        .created_at = std.time.milliTimestamp(),
    };
    const new_event_id = hx.ctx.queue.xaddZombieEvent(envelope) catch |err| {
        log.err("github_webhook.enqueue_failed zombie_id={s} delivery={s} err={s}", .{ zombie_id, delivery, @errorName(err) });
        common.internalOperationError(hx.res, "Failed to enqueue event", hx.req_id);
        return;
    };
    defer hx.alloc.free(new_event_id);

    recordAccepted(hx.ctx.telemetry, zombie.workspace_id, zombie_id, delivery);
    log.info("github_webhook.accepted zombie_id={s} delivery={s} stream_event_id={s}", .{ zombie_id, delivery, new_event_id });
    hx.ok(.accepted, .{ .status = ec.STATUS_ACCEPTED, .event_id = new_event_id });
}

fn claimDeliveryKey(hx: Hx, zombie_id: []const u8, delivery: []const u8) bool {
    var key_buf: [256]u8 = undefined;
    const key = std.fmt.bufPrint(&key_buf, "webhook:dedup:{s}:{s}:{s}", .{ zombie_id, PROVIDER_DEDUP_NAMESPACE, delivery }) catch {
        common.internalOperationError(hx.res, "dedup key overflow", hx.req_id);
        return false;
    };
    const is_new = hx.ctx.queue.setNx(key, "1", ec.DEDUP_TTL_SECONDS) catch |err| {
        log.err("github_webhook.dedup_error zombie_id={s} delivery={s} err={s}", .{ zombie_id, delivery, @errorName(err) });
        common.internalOperationError(hx.res, "Idempotency check failed", hx.req_id);
        return false;
    };
    if (!is_new) {
        log.debug("github_webhook.duplicate zombie_id={s} delivery={s}", .{ zombie_id, delivery });
        hx.ok(.ok, .{ .deduped = true });
        return false;
    }
    return true;
}

fn filterAction(alloc: std.mem.Allocator, body: []const u8) ?FilterDecision {
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, body, .{}) catch return null;
    defer parsed.deinit();
    const root = switch (parsed.value) {
        .object => |o| o,
        else => return null,
    };
    const action = stringField(root.get("action")) orelse "";
    if (!std.mem.eql(u8, action, ACTION_COMPLETED)) {
        return .{ .ingest = false, .reason = "non_completed_action" };
    }
    const wr = switch (root.get("workflow_run") orelse return null) {
        .object => |o| o,
        else => return null,
    };
    const conclusion = stringField(wr.get("conclusion")) orelse "";
    if (!std.mem.eql(u8, conclusion, CONCLUSION_FAILURE)) {
        return .{ .ingest = false, .reason = "non_failure_conclusion" };
    }
    return .{ .ingest = true, .reason = "" };
}

fn stringField(v: ?std.json.Value) ?[]const u8 {
    const val = v orelse return null;
    return switch (val) {
        .string => |s| s,
        else => null,
    };
}

const ZombieRow = struct {
    workspace_id: []const u8,
    status: []const u8,
};

fn deinitZombieRow(row: *const ZombieRow, alloc: std.mem.Allocator) void {
    alloc.free(row.workspace_id);
    alloc.free(row.status);
}

fn fetchZombieById(pool: *pg.Pool, alloc: std.mem.Allocator, zombie_id: []const u8) !?ZombieRow {
    const conn = try pool.acquire();
    defer pool.release(conn);
    var q = PgQuery.from(try conn.query(
        \\SELECT workspace_id::text, status FROM core.zombies WHERE id = $1::uuid
    , .{zombie_id}));
    defer q.deinit();
    const row = try q.next() orelse return null;
    const workspace_id = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(workspace_id);
    const status = try alloc.dupe(u8, try row.get([]const u8, 1));
    return .{ .workspace_id = workspace_id, .status = status };
}

fn recordAccepted(
    tel: *telemetry_mod.Telemetry,
    workspace_id: []const u8,
    zombie_id: []const u8,
    delivery: []const u8,
) void {
    metrics_counters.incZombiesTriggered();
    tel.capture(telemetry_mod.ZombieTriggered, .{
        .distinct_id = workspace_id,
        .workspace_id = workspace_id,
        .zombie_id = zombie_id,
        .event_id = delivery,
        .source = "github",
    });
}

// ── Tests ───────────────────────────────────────────────────────────────────
// Integration tests that exercise the full HTTP → middleware → Redis → XADD
// path live in webhook_http_integration_test.zig (TRACKED SKIP today, wires
// up once the harness gains Redis support). The unit tests below cover the
// pieces that don't need a live request.

const testing = std.testing;

test "filterAction: completed + failure → ingest" {
    const body =
        \\{"action":"completed","workflow_run":{"conclusion":"failure"}}
    ;
    const got = filterAction(testing.allocator, body) orelse return error.TestUnexpectedNull;
    try testing.expect(got.ingest);
}

test "filterAction: completed + success → ignore non_failure_conclusion" {
    const body =
        \\{"action":"completed","workflow_run":{"conclusion":"success"}}
    ;
    const got = filterAction(testing.allocator, body) orelse return error.TestUnexpectedNull;
    try testing.expect(!got.ingest);
    try testing.expectEqualStrings("non_failure_conclusion", got.reason);
}

test "filterAction: completed + cancelled → ignore non_failure_conclusion" {
    const body =
        \\{"action":"completed","workflow_run":{"conclusion":"cancelled"}}
    ;
    const got = filterAction(testing.allocator, body) orelse return error.TestUnexpectedNull;
    try testing.expect(!got.ingest);
    try testing.expectEqualStrings("non_failure_conclusion", got.reason);
}

test "filterAction: in_progress action → ignore non_completed_action" {
    const body =
        \\{"action":"in_progress","workflow_run":{"conclusion":null}}
    ;
    const got = filterAction(testing.allocator, body) orelse return error.TestUnexpectedNull;
    try testing.expect(!got.ingest);
    try testing.expectEqualStrings("non_completed_action", got.reason);
}

test "filterAction: missing action → ignore non_completed_action" {
    const body =
        \\{"workflow_run":{"conclusion":"failure"}}
    ;
    const got = filterAction(testing.allocator, body) orelse return error.TestUnexpectedNull;
    try testing.expect(!got.ingest);
    try testing.expectEqualStrings("non_completed_action", got.reason);
}

test "filterAction: missing workflow_run → null" {
    const body =
        \\{"action":"completed"}
    ;
    try testing.expect(filterAction(testing.allocator, body) == null);
}

test "filterAction: malformed JSON → null" {
    try testing.expect(filterAction(testing.allocator, "not json") == null);
}

test "filterAction: non-object root → null" {
    try testing.expect(filterAction(testing.allocator, "[1,2,3]") == null);
}

test "filterAction: parameterized non-failure conclusions" {
    const cases = [_][]const u8{
        \\{"action":"completed","workflow_run":{"conclusion":"success"}}
        ,
        \\{"action":"completed","workflow_run":{"conclusion":"neutral"}}
        ,
        \\{"action":"completed","workflow_run":{"conclusion":"skipped"}}
        ,
        \\{"action":"completed","workflow_run":{"conclusion":"timed_out"}}
        ,
        \\{"action":"completed","workflow_run":{"conclusion":"action_required"}}
        ,
    };
    for (cases) |body| {
        const got = filterAction(testing.allocator, body) orelse return error.TestUnexpectedNull;
        try testing.expect(!got.ingest);
        try testing.expectEqualStrings("non_failure_conclusion", got.reason);
    }
}

test "constants pin: MAX_BODY_BYTES is 1 MiB" {
    try testing.expectEqual(@as(usize, 1024 * 1024), MAX_BODY_BYTES);
}

test "constants pin: ACTOR + namespace + headers" {
    try testing.expectEqualStrings("webhook:github", ACTOR);
    try testing.expectEqualStrings("gh", PROVIDER_DEDUP_NAMESPACE);
    try testing.expectEqualStrings("x-github-event", HEADER_EVENT);
    try testing.expectEqualStrings("x-github-delivery", HEADER_DELIVERY);
    try testing.expectEqualStrings("workflow_run", EVENT_WORKFLOW_RUN);
    try testing.expectEqualStrings("completed", ACTION_COMPLETED);
    try testing.expectEqualStrings("failure", CONCLUSION_FAILURE);
}

test "dedupe key buffer fits the longest realistic UUID + provider + delivery" {
    // Worst case: zombie_id (36 char UUIDv7) + ":gh:" + X-GitHub-Delivery
    // (36 char UUID). "webhook:dedup:" (14) + 36 + ":gh:" (4) + 36 = 90.
    // Buffer is 256 — comfortable.
    var key_buf: [256]u8 = undefined;
    const key = try std.fmt.bufPrint(&key_buf, "webhook:dedup:{s}:gh:{s}", .{
        "01999999-9999-7999-9999-999999999999",
        "abcdef01-2345-6789-abcd-ef0123456789",
    });
    try testing.expect(key.len < 256);
    try testing.expect(std.mem.startsWith(u8, key, "webhook:dedup:"));
    try testing.expect(std.mem.indexOf(u8, key, ":gh:") != null);
}
