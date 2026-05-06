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
// Idempotency: `webhook:dedup:{zombie_id}:gh:{X-GitHub-Delivery}` (72 h TTL,
//         covers GitHub's max retry window for the same delivery UUID).
//         Dedupe is claimed AFTER zombie validation + action filter pass,
//         so 4xx-rejected deliveries and intentionally ignored events do
//         NOT consume a slot — operator-triggered redelivery of a
//         once-paused zombie processes correctly when unpaused.
// On accept: normalized envelope is XADDed to zombie:{id}:events with
//         `actor=webhook:github`, `event_type=webhook`. Returns 202.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const logging = @import("log");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const zombie_config = @import("../../../zombie/config.zig");
const telemetry_mod = @import("../../../observability/telemetry.zig");
const metrics_counters = @import("../../../observability/metrics_counters.zig");
const EventEnvelope = @import("../../../zombie/event_envelope.zig");
const normalizer = @import("../../../zombie/webhook/normalizer/github.zig");
const filter = @import("github_filter.zig");

const log = logging.scoped(.http_webhook_github);

const Hx = hx_mod.Hx;

const MAX_BODY_BYTES: usize = 1 * 1024 * 1024;
const ACTOR = "webhook:github";
const PROVIDER_DEDUP_NAMESPACE = "gh";
// 72 h covers GitHub's max retry window for the same delivery UUID.
const GITHUB_DEDUP_TTL_SECONDS: u32 = 72 * 60 * 60;
const HEADER_EVENT = "x-github-event";
const HEADER_DELIVERY = "x-github-delivery";

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

    if (!std.mem.eql(u8, event, filter.EVENT_WORKFLOW_RUN)) {
        // 200 OK + diagnostic body (not 204) so the `ignored` reason survives
        // CDNs / HTTP/2 proxies that may strip or reject 204+body per
        // RFC 9110 §6.4.5. GitHub's webhook delivery dashboard renders this
        // body when an operator inspects "Recent Deliveries".
        log.info("ignored_event", .{
            .zombie_id = zombie_id,
            .delivery = delivery,
            .event = event,
        });
        hx.ok(.ok, .{ .ignored = event });
        return;
    }

    var zombie = fetchZombieById(hx.ctx.pool, hx.alloc, zombie_id) catch |err| {
        log.err("db_error", .{
            .error_code = ec.ERR_INTERNAL_DB_QUERY,
            .zombie_id = zombie_id,
            .err = @errorName(err),
            .req_id = hx.req_id,
        });
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

    // Single parse — filter + normalize share the root on the accepted path.
    const parsed = std.json.parseFromSlice(std.json.Value, hx.alloc, body, .{}) catch |err| {
        log.warn("parse_failed", .{
            .error_code = ec.ERR_WEBHOOK_MALFORMED,
            .zombie_id = zombie_id,
            .delivery = delivery,
            .err = @errorName(err),
        });
        hx.fail(ec.ERR_WEBHOOK_MALFORMED, ec.MSG_MALFORMED_JSON);
        return;
    };
    defer parsed.deinit();
    const root: ?std.json.ObjectMap = switch (parsed.value) { .object => |o| o, else => null };
    const decision = if (root) |r| filter.filterParsedRoot(r) else null;
    if (decision == null) {
        log.warn("malformed_payload", .{
            .error_code = ec.ERR_WEBHOOK_MALFORMED,
            .zombie_id = zombie_id,
            .delivery = delivery,
        });
        hx.fail(ec.ERR_WEBHOOK_MALFORMED, ec.MSG_MALFORMED_JSON);
        return;
    }
    if (!decision.?.ingest) {
        log.info("filter_ignored", .{
            .zombie_id = zombie_id,
            .delivery = delivery,
            .reason = decision.?.reason,
        });
        hx.ok(.ok, .{ .ignored = decision.?.reason });
        return;
    }

    // Dedupe AFTER validation+filter — see file header for why.
    if (!claimDeliveryKey(hx, zombie_id, delivery)) return;

    const request_json = normalizer.normalizeFromValue(hx.alloc, root.?, std.time.timestamp()) catch |err| {
        log.err("normalize_failed", .{
            .error_code = ec.ERR_WEBHOOK_MALFORMED,
            .zombie_id = zombie_id,
            .err = @errorName(err),
            .req_id = hx.req_id,
        });
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
        log.err("enqueue_failed", .{
            .error_code = ec.ERR_INTERNAL_OPERATION_FAILED,
            .zombie_id = zombie_id,
            .delivery = delivery,
            .err = @errorName(err),
        });
        common.internalOperationError(hx.res, "Failed to enqueue event", hx.req_id);
        return;
    };
    defer hx.ctx.alloc.free(new_event_id);

    recordAccepted(hx.ctx.telemetry, zombie.workspace_id, zombie_id, delivery);
    log.info("accepted", .{
        .zombie_id = zombie_id,
        .delivery = delivery,
        .stream_event_id = new_event_id,
    });
    hx.ok(.accepted, .{ .status = ec.STATUS_ACCEPTED, .event_id = new_event_id });
}

fn claimDeliveryKey(hx: Hx, zombie_id: []const u8, delivery: []const u8) bool {
    var key_buf: [256]u8 = undefined;
    const key = std.fmt.bufPrint(&key_buf, "webhook:dedup:{s}:{s}:{s}", .{ zombie_id, PROVIDER_DEDUP_NAMESPACE, delivery }) catch {
        common.internalOperationError(hx.res, "dedup key overflow", hx.req_id);
        return false;
    };
    const is_new = hx.ctx.queue.setNx(key, "1", GITHUB_DEDUP_TTL_SECONDS) catch |err| {
        log.err("dedup_error", .{
            .error_code = ec.ERR_INTERNAL_OPERATION_FAILED,
            .zombie_id = zombie_id,
            .delivery = delivery,
            .err = @errorName(err),
        });
        common.internalOperationError(hx.res, "Idempotency check failed", hx.req_id);
        return false;
    };
    if (!is_new) {
        log.debug("duplicate", .{ .zombie_id = zombie_id, .delivery = delivery });
        hx.ok(.ok, .{ .deduped = true });
        return false;
    }
    return true;
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
// Filter logic + its tests live in `github_filter.zig` (sibling). Integration
// tests that exercise the full HTTP → middleware → Redis → XADD path live in
// webhook_http_integration_test.zig. The handler-level pin below covers
// constants the filter doesn't own.

const testing = std.testing;

test "handler constants pin" {
    try testing.expectEqual(@as(usize, 1024 * 1024), MAX_BODY_BYTES);
    try testing.expectEqual(@as(u32, 72 * 60 * 60), GITHUB_DEDUP_TTL_SECONDS);
    try testing.expectEqualStrings("webhook:github", ACTOR);
    try testing.expectEqualStrings("gh", PROVIDER_DEDUP_NAMESPACE);
    try testing.expectEqualStrings("x-github-event", HEADER_EVENT);
    try testing.expectEqualStrings("x-github-delivery", HEADER_DELIVERY);
    // Worst-case dedupe key: "webhook:dedup:" (14) + UUIDv7 (36) + ":gh:" (4)
    // + delivery UUID (36) = 90 bytes. The 256-byte buffer is comfortable.
    var key_buf: [256]u8 = undefined;
    const key = try std.fmt.bufPrint(&key_buf, "webhook:dedup:{s}:gh:{s}", .{
        "01999999-9999-7999-9999-999999999999",
        "abcdef01-2345-6789-abcd-ef0123456789",
    });
    try testing.expect(key.len < 256);
}
