const std = @import("std");
const zap = @import("zap");
const metrics = @import("../../observability/metrics.zig");
const obs_log = @import("../../observability/logging.zig");
const common = @import("common.zig");

pub const Context = common.Context;

const QueueHealth = struct {
    queued_count: i64,
    oldest_queued_age_ms: ?i64,
};

pub const ReadyInputs = struct {
    db_ok: bool,
    worker_ok: bool,
    queue_dependency_ok: bool,
    queue_depth_breached: bool,
    queue_age_breached: bool,
};

const QueueBreaches = struct {
    depth: bool,
    age: bool,
};

fn databaseHealthy(ctx: *Context) bool {
    const conn = ctx.pool.acquire() catch return false;
    defer ctx.pool.release(conn);

    var ping = conn.query("SELECT 1", .{}) catch return false;
    defer ping.deinit();

    return (ping.next() catch null) != null;
}

fn queueHealth(ctx: *Context) ?QueueHealth {
    const conn = ctx.pool.acquire() catch return null;
    defer ctx.pool.release(conn);

    var q = conn.query(
        \\SELECT COUNT(*)::BIGINT, MIN(created_at)::BIGINT
        \\FROM runs
        \\WHERE state = 'SPEC_QUEUED'
    , .{}) catch return null;
    defer q.deinit();

    const row = (q.next() catch null) orelse return null;
    const queued_count = row.get(i64, 0) catch return null;
    const oldest_created_ms = row.get(?i64, 1) catch return null;
    const now_ms = std.time.milliTimestamp();
    const oldest_age_ms = if (oldest_created_ms) |ts| now_ms - ts else null;
    return .{
        .queued_count = queued_count,
        .oldest_queued_age_ms = oldest_age_ms,
    };
}

fn queueDependencyHealthy(ctx: *Context) bool {
    ctx.queue.readyCheck() catch |err| {
        obs_log.logWarnErr(.http, err, "readyz: redis queue dependency check failed", .{});
        return false;
    };
    return true;
}

fn evaluateQueueBreaches(ctx: *Context, qh: ?QueueHealth) QueueBreaches {
    if (qh) |v| {
        const depth = if (ctx.ready_max_queue_depth) |limit| v.queued_count > limit else false;
        const age = if (ctx.ready_max_queue_age_ms) |limit|
            if (v.oldest_queued_age_ms) |queued_age_ms| queued_age_ms > limit else false
        else
            false;
        return .{ .depth = depth, .age = age };
    }
    return .{ .depth = false, .age = false };
}

pub fn readyDecision(inputs: ReadyInputs) bool {
    return inputs.db_ok and
        inputs.worker_ok and
        inputs.queue_dependency_ok and
        !inputs.queue_depth_breached and
        !inputs.queue_age_breached;
}

pub fn handleHealthz(ctx: *Context, r: zap.Request) void {
    const db_ok = databaseHealthy(ctx);
    if (!db_ok) {
        common.writeJson(r, .service_unavailable, .{
            .status = "degraded",
            .service = "zombied",
            .database = "down",
        });
        return;
    }

    common.writeJson(r, .ok, .{
        .status = "ok",
        .service = "zombied",
        .database = "up",
    });
}

pub fn handleReadyz(ctx: *Context, r: zap.Request) void {
    const db_ok = databaseHealthy(ctx);
    const worker_ok = ctx.worker_state.running.load(.acquire);
    const queue_dependency_ok = queueDependencyHealthy(ctx);
    const qh = if (db_ok) queueHealth(ctx) else null;
    const breaches = evaluateQueueBreaches(ctx, qh);

    if (!readyDecision(.{
        .db_ok = db_ok,
        .worker_ok = worker_ok,
        .queue_dependency_ok = queue_dependency_ok,
        .queue_depth_breached = breaches.depth,
        .queue_age_breached = breaches.age,
    })) {
        common.writeJson(r, .service_unavailable, .{
            .ready = false,
            .database = db_ok,
            .worker = worker_ok,
            .queue_dependency = queue_dependency_ok,
            .queue_depth = if (qh) |v| v.queued_count else null,
            .oldest_queued_age_ms = if (qh) |v| v.oldest_queued_age_ms else null,
            .queue_depth_breached = breaches.depth,
            .queue_age_breached = breaches.age,
            .queue_depth_limit = ctx.ready_max_queue_depth,
            .queue_age_limit_ms = ctx.ready_max_queue_age_ms,
        });
        return;
    }

    common.writeJson(r, .ok, .{
        .ready = true,
        .database = true,
        .worker = true,
        .queue_dependency = true,
        .queue_depth = if (qh) |v| v.queued_count else @as(i64, 0),
        .oldest_queued_age_ms = if (qh) |v| v.oldest_queued_age_ms else null,
        .queue_depth_breached = false,
        .queue_age_breached = false,
        .queue_depth_limit = ctx.ready_max_queue_depth,
        .queue_age_limit_ms = ctx.ready_max_queue_age_ms,
    });
}

pub fn handleMetrics(ctx: *Context, r: zap.Request) void {
    const qh = queueHealth(ctx);
    const body = metrics.renderPrometheus(
        ctx.alloc,
        ctx.worker_state.running.load(.acquire),
        if (qh) |v| v.queued_count else null,
        if (qh) |v| v.oldest_queued_age_ms else null,
    ) catch {
        r.setStatus(.internal_server_error);
        r.sendBody("") catch |err| obs_log.logWarnErr(.http, err, "metrics send failed", .{});
        return;
    };
    defer ctx.alloc.free(body);

    r.setStatus(.ok);
    r.setContentType(.TEXT) catch |err| obs_log.logWarnErr(.http, err, "setContentType TEXT failed", .{});
    r.sendBody(body) catch |err| obs_log.logWarnErr(.http, err, "metrics body send failed", .{});
}

test "integration: ready decision fails closed when redis queue dependency is degraded" {
    try std.testing.expect(!readyDecision(.{
        .db_ok = true,
        .worker_ok = true,
        .queue_dependency_ok = false,
        .queue_depth_breached = false,
        .queue_age_breached = false,
    }));
}

test "integration: ready decision fails during worker restart window" {
    try std.testing.expect(!readyDecision(.{
        .db_ok = true,
        .worker_ok = false,
        .queue_dependency_ok = true,
        .queue_depth_breached = false,
        .queue_age_breached = false,
    }));
}

test "integration: ready decision passes when dependencies and guardrails are healthy" {
    try std.testing.expect(readyDecision(.{
        .db_ok = true,
        .worker_ok = true,
        .queue_dependency_ok = true,
        .queue_depth_breached = false,
        .queue_age_breached = false,
    }));
}

test "integration: queue breach evaluator flags depth breach when queue exceeds limit" {
    var ctx = std.mem.zeroes(Context);
    ctx.ready_max_queue_depth = 5;
    const breaches = evaluateQueueBreaches(&ctx, .{ .queued_count = 6, .oldest_queued_age_ms = null });
    try std.testing.expectEqual(true, breaches.depth);
    try std.testing.expectEqual(false, breaches.age);
}

test "integration: queue breach evaluator flags age breach when oldest age exceeds limit" {
    var ctx = std.mem.zeroes(Context);
    ctx.ready_max_queue_age_ms = 1000;
    const breaches = evaluateQueueBreaches(&ctx, .{ .queued_count = 1, .oldest_queued_age_ms = 1500 });
    try std.testing.expectEqual(false, breaches.depth);
    try std.testing.expectEqual(true, breaches.age);
}

test "integration: queue breach evaluator stays open when queue metrics are unavailable" {
    var ctx = std.mem.zeroes(Context);
    ctx.ready_max_queue_depth = 2;
    ctx.ready_max_queue_age_ms = 1000;
    const breaches = evaluateQueueBreaches(&ctx, null);
    try std.testing.expectEqual(false, breaches.depth);
    try std.testing.expectEqual(false, breaches.age);
}
