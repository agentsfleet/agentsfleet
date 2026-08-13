//! Bounded retry loop from durable repair-verification intents to Fleet events.

const std = @import("std");
const logging = @import("log");
const pg = @import("pg");
const common_lib = @import("common");
const clock = common_lib.clock;

const ec = @import("../errors/error_registry.zig");
const queue_redis = @import("../queue/redis_client.zig");
const repair_verification_queue = @import("../queue/redis_repair_verification.zig");
const repair_verifications = @import("../state/repair_verifications.zig");
const metrics = @import("../observability/metrics_repair_verification.zig");

const log = logging.scoped(.repair_verification_dispatcher);

const SWEEP_INTERVAL_NS: u64 = std.time.ns_per_min;
const FAILED_BATCH_RETRY_NS: u64 = @as(u64, @intCast(repair_verifications.CLAIM_STALE_MS)) * std.time.ns_per_ms;
const SHUTDOWN_POLL_NS: u64 = std.time.ns_per_s;
const CLEANUP_BURST_LIMIT: usize = 8;
const CLEANUP_BURST_PAUSE_NS: u64 = 100 * std.time.ns_per_ms;
const LOG_CLEANUP_LOOKUP_FAILED = "repair_verification_cleanup_lookup_failed";
const LOG_CLEANUP_UPDATE_FAILED = "repair_verification_cleanup_update_failed";

pub const Stats = struct {
    due: usize = 0,
    completed: usize = 0,
    failed: usize = 0,
    cleanup_pending: bool = false,
};

const CleanupStats = struct {
    attempted: usize = 0,
    completed: usize = 0,
};

const DispatchOutcome = struct {
    completed: bool,
    replayed: bool,
    queued_at_ms: i64,
};

const SyntheticEvent = struct {
    event_type: []const u8 = repair_verifications.SYNTHETIC_EVENT,
    incident: struct {
        fleet_id: []const u8,
        event_id: []const u8,
        request_json: []const u8,
        response_text: []const u8,
    },
    repair: struct {
        pr_number: i64,
        pr_url: []const u8,
        merged_commit_sha: []const u8,
        merged_at: i64,
    },
    production: struct {
        provider: []const u8,
        deployment_id: []const u8,
        conclusion: []const u8,
        completed_at: i64,
    },
    evidence_window: struct {
        start_at: i64,
        end_at: i64,
    },
};

/// Run until daemon shutdown. Each pass owns at most `DUE_BATCH_LIMIT` due
/// intents, so a backlog does not turn one background thread into a queue.
pub fn run(pool: *pg.Pool, queue: *queue_redis.Client, alloc: std.mem.Allocator, shutdown: *std.atomic.Value(bool)) void {
    log.debug("repair_verification_dispatcher_started", .{ .batch_limit = repair_verifications.DUE_BATCH_LIMIT });
    var cleanup_pages: usize = 0;
    while (!shutdown.load(.acquire)) { // safe because: serve shutdown release-store pairs with this acquire-load.
        const stats = dispatchOnce(pool, queue, alloc, clock.nowMillis()) catch |err| {
            log.warn("repair_verification_dispatch_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
            sleepInterruptible(shutdown, SWEEP_INTERVAL_NS);
            continue;
        };
        if (stats.completed > 0) log.info("repair_verification_dispatched", .{ .due = stats.due, .completed = stats.completed, .failed = stats.failed });
        if (stats.cleanup_pending) {
            cleanup_pages += 1;
            if (cleanupBurstPauseDue(cleanup_pages)) {
                sleepInterruptible(shutdown, CLEANUP_BURST_PAUSE_NS);
                cleanup_pages = 0;
            }
            continue;
        }
        cleanup_pages = 0;
        if (sleepAfter(stats)) |duration| sleepInterruptible(shutdown, duration);
    }
    log.debug("repair_verification_dispatcher_stopped", .{});
}

/// Execute one due batch. Public for integration coverage of the time and
/// crash-boundary behaviour without starting an unbounded daemon thread.
pub fn dispatchOnce(pool: *pg.Pool, queue: *queue_redis.Client, alloc: std.mem.Allocator, now_ms: i64) !Stats {
    var batch = blk: {
        const conn = try pool.acquire();
        defer pool.release(conn);
        break :blk try repair_verifications.claimDue(alloc, conn, now_ms);
    };
    defer batch.deinit(alloc);
    var stats = Stats{ .due = batch.items.len };
    const oldest_age_ms = if (batch.items.len == 0) 0 else @max(0, now_ms - batch.items[0].verify_after);
    metrics.observeDispatchDueBatch(batch.items.len, oldest_age_ms);
    for (batch.items) |item| {
        const outcome = dispatchItem(pool, queue, alloc, item, batch.token, now_ms) catch |err| {
            stats.failed += 1;
            metrics.incDispatchRetried();
            log.warn("repair_verification_dispatch_item_failed", .{
                .error_code = ec.ERR_INTERNAL_OPERATION_FAILED,
                .verification_id = item.id,
                .workspace_id = item.workspace_id,
                .repository = item.repository,
                .provider_deployment_id = item.provider_deployment_id,
                .commit = item.merged_commit_sha[0..@min(item.merged_commit_sha.len, 12)],
                .repair_link_id = item.repair_link_id,
                .err = @errorName(err),
            });
            continue;
        };
        if (outcome.completed) {
            stats.completed += 1;
            metrics.observeEventQueued(outcome.replayed, item.completed_at, outcome.queued_at_ms);
        } else {
            stats.failed += 1;
            metrics.incDispatchRetried();
        }
    }
    const cleanup = cleanCompletedOnceKeys(pool, queue, alloc, now_ms);
    stats.cleanup_pending = cleanup.attempted == @as(usize, @intCast(repair_verifications.REDIS_CLEANUP_BATCH_LIMIT)) and
        cleanup.completed > 0;
    return stats;
}

fn dispatchItem(pool: *pg.Pool, queue: *queue_redis.Client, alloc: std.mem.Allocator, item: repair_verifications.Due, claim_token: []const u8, now_ms: i64) !DispatchOutcome {
    const request_json = try eventJson(alloc, item);
    defer alloc.free(request_json);
    const enqueue = try repair_verification_queue.xaddOnce(queue, item.id, .{
        .event_id = "",
        .fleet_id = item.verifier_fleet_id,
        .workspace_id = item.workspace_id,
        .actor = repair_verifications.VERIFIER_EVENT_ACTOR,
        .event_type = .webhook,
        .request_json = request_json,
        .created_at = now_ms,
    });
    defer queue.alloc.free(enqueue.event_id);
    const completed = blk: {
        const conn = try pool.acquire();
        defer pool.release(conn);
        break :blk try repair_verifications.complete(conn, item.id, claim_token, enqueue.event_id, now_ms);
    };
    if (completed) log.info("repair_verification_event_emitted", .{
        .verification_id = item.id,
        .workspace_id = item.workspace_id,
        .repository = item.repository,
        .provider_deployment_id = item.provider_deployment_id,
        .commit = item.merged_commit_sha[0..@min(item.merged_commit_sha.len, 12)],
        .repair_link_id = item.repair_link_id,
        .verifier_event_id = enqueue.event_id,
    });
    return .{
        .completed = completed,
        .replayed = enqueue.replayed,
        .queued_at_ms = enqueue.queued_at_ms,
    };
}

fn sleepAfter(stats: Stats) ?u64 {
    if (stats.cleanup_pending) return null;
    const full_batch = stats.due == @as(usize, @intCast(repair_verifications.DUE_BATCH_LIMIT));
    if (stats.failed > 0 and (!full_batch or stats.completed == 0)) return FAILED_BATCH_RETRY_NS;
    if (!full_batch) return SWEEP_INTERVAL_NS;
    return null;
}

fn cleanupBurstPauseDue(cleanup_pages: usize) bool {
    return cleanup_pages >= CLEANUP_BURST_LIMIT;
}

fn cleanCompletedOnceKeys(pool: *pg.Pool, queue: *queue_redis.Client, alloc: std.mem.Allocator, now_ms: i64) CleanupStats {
    const rows = blk: {
        const conn = pool.acquire() catch |err| {
            log.warn(LOG_CLEANUP_LOOKUP_FAILED, .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
            return .{};
        };
        defer pool.release(conn);
        break :blk repair_verifications.redisCleanupDue(alloc, conn, now_ms) catch |err| {
            log.warn(LOG_CLEANUP_LOOKUP_FAILED, .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
            return .{};
        };
    };
    defer repair_verifications.freeRedisCleanup(alloc, rows);
    var cleared: std.ArrayList([]const u8) = .empty;
    defer cleared.deinit(alloc);
    for (rows) |item| {
        repair_verification_queue.clearOnce(queue, item.id) catch |err| {
            log.warn("repair_verification_cleanup_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .verification_id = item.id, .err = @errorName(err) });
            continue;
        };
        cleared.append(alloc, item.id) catch |err| {
            log.warn(LOG_CLEANUP_UPDATE_FAILED, .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
        };
    }
    if (cleared.items.len == 0) return .{ .attempted = rows.len };
    const conn = pool.acquire() catch |err| {
        log.warn(LOG_CLEANUP_UPDATE_FAILED, .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
        return .{ .attempted = rows.len };
    };
    defer pool.release(conn);
    const completed = repair_verifications.completeRedisCleanup(alloc, conn, cleared.items, now_ms) catch |err| {
        log.warn(LOG_CLEANUP_UPDATE_FAILED, .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
        return .{ .attempted = rows.len };
    };
    return .{ .attempted = rows.len, .completed = completed };
}

fn eventJson(alloc: std.mem.Allocator, due: repair_verifications.Due) ![]u8 {
    return std.json.Stringify.valueAlloc(alloc, SyntheticEvent{
        .incident = .{
            .fleet_id = due.incident_fleet_id,
            .event_id = due.incident_event_id,
            .request_json = due.incident_request_json,
            .response_text = due.incident_response_text,
        },
        .repair = .{
            .pr_number = due.pr_number,
            .pr_url = due.pr_url,
            .merged_commit_sha = due.merged_commit_sha,
            .merged_at = due.merged_at,
        },
        .production = .{
            .provider = due.provider,
            .deployment_id = due.provider_deployment_id,
            .conclusion = due.conclusion,
            .completed_at = due.completed_at,
        },
        .evidence_window = .{
            .start_at = due.completed_at,
            .end_at = due.verify_after,
        },
    }, .{});
}

fn sleepInterruptible(shutdown: *std.atomic.Value(bool), total_ns: u64) void {
    var remaining = total_ns;
    while (remaining > 0) {
        if (shutdown.load(.acquire)) return;
        const step = @min(remaining, SHUTDOWN_POLL_NS);
        common_lib.sleepNanos(step);
        remaining -|= step;
    }
}

test "test_dispatcher_event_uses_exact_merge_hash" {
    const due = repair_verifications.Due{
        .id = "0195c102-7000-7000-8000-000000000001",
        .repair_link_id = "0195c102-7000-7000-8000-000000000005",
        .repository = "agentsfleet/agentsfleet",
        .workspace_id = "0195c102-7000-7000-8000-000000000002",
        .verifier_fleet_id = "0195c102-7000-7000-8000-000000000003",
        .incident_fleet_id = "0195c102-7000-7000-8000-000000000004",
        .incident_event_id = "incident-event",
        .incident_request_json = "{\"symptom\":\"latency\"}",
        .incident_response_text = "The latency alert began after deploy 17.",
        .pr_number = 12,
        .pr_url = "https://github.com/agentsfleet/agentsfleet/pull/12",
        .merged_commit_sha = "exact-merge-hash",
        .merged_at = 10,
        .provider = "github",
        .provider_deployment_id = "42",
        .conclusion = "success",
        .completed_at = 20,
        .verify_after = 30,
    };
    const body = try eventJson(std.testing.allocator, due);
    defer std.testing.allocator.free(body);
    try std.testing.expect(std.mem.indexOf(u8, body, "repair_production_result") != null);
    try std.testing.expect(std.mem.indexOf(u8, body, "exact-merge-hash") != null);
    try std.testing.expect(std.mem.indexOf(u8, body, "latency alert began") != null);
    try std.testing.expect(std.mem.indexOf(u8, body, "current_default_branch") == null);
}

test "full failed batch waits for the stale-claim fence" {
    const limit: usize = @intCast(repair_verifications.DUE_BATCH_LIMIT);
    try std.testing.expectEqual(FAILED_BATCH_RETRY_NS, sleepAfter(.{ .due = limit, .failed = limit }).?);
    try std.testing.expectEqual(FAILED_BATCH_RETRY_NS, sleepAfter(.{ .due = 1, .failed = 1 }).?);
    try std.testing.expect(sleepAfter(.{ .due = limit, .completed = limit - 1, .failed = 1 }) == null);
    try std.testing.expect(sleepAfter(.{ .due = limit, .completed = limit }) == null);
    try std.testing.expectEqual(SWEEP_INTERVAL_NS, sleepAfter(.{ .due = 0 }).?);
    try std.testing.expect(sleepAfter(.{ .cleanup_pending = true }) == null);
    try std.testing.expect(!cleanupBurstPauseDue(CLEANUP_BURST_LIMIT - 1));
    try std.testing.expect(cleanupBurstPauseDue(CLEANUP_BURST_LIMIT));
}
