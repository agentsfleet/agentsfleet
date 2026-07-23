// POST /v1/webhooks/{fleet_id}/github — signed GitHub webhook ingest.
//
// Auth: HMAC-SHA256 over the raw body (X-Hub-Signature-256), verified by the
//       webhook_sig middleware against the workspace's `github`
//       credential. This handler runs only after the signature is valid.
//
// Body cap: 1 MiB (UZ-WH-030 before any other work).
// Filter: reviewable `pull_request` events and failed completed
//         `workflow_run` events are added; everything else returns 200 OK
//         with a `{"ignored":"<reason>"}` body so the diagnostic survives
//         CDN / HTTP/2 proxy paths (RFC 9110 §6.4.5 forbids 204+body).
// Idempotency: `webhook:dedup:{fleet_id}:gh:{X-GitHub-Delivery}` (72 h TTL,
//         covers GitHub's max retry window for the same delivery UUID).
//         The slot is claimed atomically (SET NX EX — concurrent duplicate
//         deliveries still single-enqueue) AFTER fleet validation + action
//         filter pass, so 4xx-rejected and intentionally ignored deliveries
//         never consume it, and RELEASED (DEL) on every post-claim failure
//         path — normalize failure included — so a transient fault leaves
//         GitHub's redelivery deliverable (loss-proof dedup ordering).
// On accept: normalized envelope is XADDed to fleet:{id}:events with
//         `actor=webhook:github`, `event_type=webhook`. Returns 202.

const std = @import("std");
const sql = @import("sql.zig");
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
const normalizer = @import("../../../fleet_runtime/webhook/normalizer/github_app.zig");
const filter = @import("github_filter.zig");

const S_FLEET_ID_MUST_BE_UUIDV7 = "fleet_id must be a valid UUIDv7";
const BYTES_PER_KIB = 1024;

const log = logging.scoped(.http_webhook_github);

const Hx = hx_mod.Hx;

const S_WEBHOOK_BODY_EXCEEDS_1_MIB = "Webhook body exceeds 1 MiB";

const MAX_BODY_BYTES: usize = 1 * 1024 * 1024;
const ACTOR = "webhook:github";
const PROVIDER_DEDUP_NAMESPACE = "gh";
// 72 h covers GitHub's max retry window for the same delivery UUID.
const GITHUB_DEDUP_TTL_SECONDS: u32 = 72 * 60 * 60;
const HEADER_EVENT = "x-github-event";
const HEADER_DELIVERY = "x-github-delivery";

pub fn innerInvokeGithubWebhook(hx: Hx, req: *httpz.Request, fleet_id: []const u8) void {
    // The fleet id lands in a case-sensitive Redis dedup key below while
    // `WHERE id = $1::uuid` folds case in Postgres. Without this guard the same
    // delivery under two spellings resolves to one fleet but two dedup slots,
    // and gets processed twice. Validate before the id is used as a key.
    if (!id_format.isUuidV7(fleet_id)) {
        hx.fail(ec.ERR_UUIDV7_INVALID_ID_SHAPE, S_FLEET_ID_MUST_BE_UUIDV7);
        return;
    }
    // Pre-read fence: reject oversized payloads before httpz buffers them.
    // The httpz server-level max_body_size may be larger than our 1 MiB cap,
    // so without this guard a >1 MiB body would be fully buffered + discarded.
    if (req.header("content-length")) |cl_str| {
        const cl = std.fmt.parseInt(usize, cl_str, 10) catch 0;
        if (cl > MAX_BODY_BYTES) {
            hx.fail(ec.ERR_WEBHOOK_PAYLOAD_TOO_LARGE, S_WEBHOOK_BODY_EXCEEDS_1_MIB);
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
        hx.fail(ec.ERR_WEBHOOK_PAYLOAD_TOO_LARGE, S_WEBHOOK_BODY_EXCEEDS_1_MIB);
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

    if (!filter.isSupportedEvent(event)) {
        // 200 OK + diagnostic body (not 204) so the `ignored` reason survives
        // CDNs / HTTP/2 proxies that may strip or reject 204+body per
        // RFC 9110 §6.4.5. GitHub's webhook delivery dashboard renders this
        // body when an operator inspects "Recent Deliveries".
        log.debug("ignored_event", .{
            .fleet_id = fleet_id,
            .delivery = delivery,
            .event = event,
        });
        hx.ok(.ok, .{ .ignored = event });
        return;
    }

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
        hx.fail(ec.ERR_WEBHOOK_NO_AGENT, ec.MSG_AGENTSFLEET_NOT_FOUND);
        return;
    };
    defer deinitFleetRow(&fleet, hx.alloc);

    // Paused fleet → 200-ignored, not 4xx: GitHub retry queues add no value
    // for an intentionally paused fleet, and the dedup slot is NOT consumed
    // so an operator redelivery after resume processes correctly.
    // The triggered metric is not incremented — nothing was accepted.
    const status = fleet_config.FleetStatus.fromSlice(fleet.status) orelse .stopped;
    if (!status.isRunnable()) {
        log.debug("fleet_not_active", .{
            .fleet_id = fleet_id,
            .status = fleet.status,
            .delivery = delivery,
        });
        hx.ok(.ok, .{ .ignored = ec.IGNORED_REASON_AGENTSFLEET_PAUSED });
        return;
    }

    // Single parse — filter + normalize share the root on the accepted path.
    const parsed = std.json.parseFromSlice(std.json.Value, hx.alloc, body, .{}) catch |err| {
        log.warn("parse_failed", .{
            .error_code = ec.ERR_WEBHOOK_MALFORMED,
            .fleet_id = fleet_id,
            .delivery = delivery,
            .err = @errorName(err),
        });
        hx.fail(ec.ERR_WEBHOOK_MALFORMED, ec.MSG_MALFORMED_JSON);
        return;
    };
    defer parsed.deinit();
    const root: ?std.json.ObjectMap = switch (parsed.value) {
        .object => |o| o,
        else => null,
    };
    const decision = if (root) |r| filter.filterParsedRoot(event, r) else null;
    if (decision == null) {
        log.warn("malformed_payload", .{
            .error_code = ec.ERR_WEBHOOK_MALFORMED,
            .fleet_id = fleet_id,
            .delivery = delivery,
        });
        hx.fail(ec.ERR_WEBHOOK_MALFORMED, ec.MSG_MALFORMED_JSON);
        return;
    }
    if (!decision.?.ingest) {
        log.debug("filter_ignored", .{
            .fleet_id = fleet_id,
            .delivery = delivery,
            .reason = decision.?.reason,
        });
        hx.ok(.ok, .{ .ignored = decision.?.reason });
        return;
    }

    // Atomic claim after validation+filter; released on every post-claim
    // failure below — see file header for why.
    var dedup_key_buf: [256]u8 = undefined;
    const dedup_key = std.fmt.bufPrint(&dedup_key_buf, "{s}{s}:{s}:{s}", .{ ec.WEBHOOK_DEDUP_KEY_PREFIX, fleet_id, PROVIDER_DEDUP_NAMESPACE, delivery }) catch {
        common.internalOperationError(hx.res, "Failed to build the duplicate-event check", hx.req_id);
        return;
    };
    if (!claimDedupSlot(hx, fleet_id, delivery, dedup_key)) return;

    const normalized = normalizer.normalizeFromValue(hx.alloc, event, root.?, clock.nowSeconds()) catch |err| {
        // Normalize failure must not burn the slot: GitHub's redelivery of a
        // (possibly fixed) payload for this delivery UUID stays deliverable.
        releaseDedupSlot(hx, fleet_id, dedup_key);
        log.err("normalize_failed", .{
            .error_code = ec.ERR_WEBHOOK_MALFORMED,
            .fleet_id = fleet_id,
            .err = @errorName(err),
            .req_id = hx.req_id,
        });
        hx.fail(ec.ERR_WEBHOOK_MALFORMED, ec.MSG_MALFORMED_JSON);
        return;
    };
    const request_json = switch (normalized) {
        .accepted => |json| json,
        .ignored => |reason| {
            releaseDedupSlot(hx, fleet_id, dedup_key);
            hx.ok(.ok, .{ .ignored = reason });
            return;
        },
    };
    defer hx.alloc.free(request_json);

    const envelope = EventEnvelope{
        .event_id = "",
        .fleet_id = fleet_id,
        .workspace_id = fleet.workspace_id,
        .actor = ACTOR,
        .event_type = .webhook,
        .request_json = request_json,
        .created_at = clock.nowMillis(),
    };
    const new_event_id = hx.ctx.queue.xaddFleetEvent(envelope) catch |err| {
        // Release the slot — GitHub's redelivery of this UUID stays
        // deliverable (loss-proof dedup ordering).
        releaseDedupSlot(hx, fleet_id, dedup_key);
        log.err("enqueue_failed", .{
            .error_code = ec.ERR_INTERNAL_OPERATION_FAILED,
            .fleet_id = fleet_id,
            .delivery = delivery,
            .err = @errorName(err),
        });
        common.internalOperationError(hx.res, "Failed to enqueue event", hx.req_id);
        return;
    };
    defer hx.ctx.alloc.free(new_event_id);

    recordAccepted(hx.ctx.telemetry, fleet.workspace_id, fleet_id, delivery);
    log.debug("accepted", .{
        .fleet_id = fleet_id,
        .delivery = delivery,
        .stream_event_id = new_event_id,
    });
    hx.ok(.accepted, .{ .status = ec.STATUS_ACCEPTED, .event_id = new_event_id });
}

/// Atomically claim the delivery's idempotency slot (SET NX — exactly one of
/// N concurrent identical deliveries wins). The caller releases it on every
/// post-claim failure path.
fn claimDedupSlot(hx: Hx, fleet_id: []const u8, delivery: []const u8, dedup_key: []const u8) bool {
    const is_new = hx.ctx.queue.setNx(dedup_key, "1", GITHUB_DEDUP_TTL_SECONDS) catch |err| {
        log.err("dedup_error", .{
            .error_code = ec.ERR_INTERNAL_OPERATION_FAILED,
            .fleet_id = fleet_id,
            .delivery = delivery,
            .err = @errorName(err),
        });
        common.internalOperationError(hx.res, "Failed to check for a duplicate event", hx.req_id);
        return false;
    };
    if (!is_new) {
        log.debug("duplicate", .{ .fleet_id = fleet_id, .delivery = delivery });
        hx.ok(.ok, .{ .deduped = true });
        return false;
    }
    return true;
}

/// Release a claimed idempotency slot after a post-claim failure so GitHub's
/// redelivery is not answered "duplicate" for an event that never landed.
/// Best-effort: on a DEL failure the slot expires at its TTL (logged).
fn releaseDedupSlot(hx: Hx, fleet_id: []const u8, dedup_key: []const u8) void {
    hx.ctx.queue.del(dedup_key) catch |err| {
        log.warn("dedup_release_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = fleet_id, .err = @errorName(err) });
    };
}

const FleetRow = struct {
    workspace_id: []const u8,
    status: []const u8,
};

fn deinitFleetRow(row: *const FleetRow, alloc: std.mem.Allocator) void {
    alloc.free(row.workspace_id);
    alloc.free(row.status);
}

fn fetchFleetById(pool: *pg.Pool, alloc: std.mem.Allocator, fleet_id: []const u8) !?FleetRow {
    const conn = try pool.acquire();
    defer pool.release(conn);
    var q = PgQuery.from(try conn.query(sql.SELECT_FLEET_WORKSPACE_AND_STATUS, .{fleet_id}));
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
    fleet_id: []const u8,
    delivery: []const u8,
) void {
    metrics_counters.incFleetsTriggered();
    tel.capture(telemetry_mod.FleetTriggered, .{
        .distinct_id = workspace_id,
        .workspace_id = workspace_id,
        .fleet_id = fleet_id,
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
    try testing.expectEqual(@as(usize, BYTES_PER_KIB * BYTES_PER_KIB), MAX_BODY_BYTES);
    try testing.expectEqual(@as(u32, 72 * 60 * 60), GITHUB_DEDUP_TTL_SECONDS);
    try testing.expectEqualStrings("webhook:github", ACTOR);
    try testing.expectEqualStrings("gh", PROVIDER_DEDUP_NAMESPACE);
    try testing.expectEqualStrings("x-github-event", HEADER_EVENT);
    try testing.expectEqualStrings("x-github-delivery", HEADER_DELIVERY);
    // Worst-case dedupe key: WEBHOOK_DEDUP_KEY_PREFIX (14) + UUIDv7 (36) +
    // ":gh:" (4) + delivery UUID (36) = 90 bytes. 256-byte buffer is comfortable.
    var key_buf: [256]u8 = undefined;
    const key = try std.fmt.bufPrint(&key_buf, "{s}{s}:{s}:{s}", .{
        ec.WEBHOOK_DEDUP_KEY_PREFIX,
        "01999999-9999-7999-9999-999999999999",
        PROVIDER_DEDUP_NAMESPACE,
        "abcdef01-2345-6789-abcd-ef0123456789",
    });
    try testing.expect(key.len < 256);
}
