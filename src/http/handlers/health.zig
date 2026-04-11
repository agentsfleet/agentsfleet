const std = @import("std");
const httpz = @import("httpz");
const metrics = @import("../../observability/metrics.zig");
const obs_log = @import("../../observability/logging.zig");
const common = @import("common.zig");
const build_options = @import("build_options");

pub const Context = common.Context;

// M10_001: QueueHealth struct and queueHealth() removed — they queried the
// dropped `runs` table for SPEC_QUEUED count. Zombie uses Redis streams,
// not DB-level queue depth. Queue depth/age metrics are no longer emitted.

pub const ReadyInputs = struct {
    db_ok: bool,
    queue_dependency_ok: bool,
};

fn databaseHealthy(ctx: *Context) bool {
    const conn = ctx.pool.acquire() catch return false;
    defer ctx.pool.release(conn);

    var ping = conn.query("SELECT 1", .{}) catch return false;
    defer ping.deinit();

    const alive = (ping.next() catch null) != null;
    ping.drain() catch {};
    return alive;
}

fn queueDependencyHealthy(ctx: *Context) bool {
    ctx.queue.readyCheck() catch |err| {
        obs_log.logWarnErr(.http, err, "readyz: redis queue dependency check failed", .{});
        return false;
    };
    return true;
}

pub fn readyDecision(inputs: ReadyInputs) bool {
    return inputs.db_ok and inputs.queue_dependency_ok;
}

pub fn handleHealthz(ctx: *Context, req: *httpz.Request, res: *httpz.Response) void {
    _ = req;
    const db_ok = databaseHealthy(ctx);
    if (!db_ok) {
        common.writeJson(res, .service_unavailable, .{
            .status = "degraded",
            .service = "zombied",
            .database = "down",
        });
        return;
    }

    common.writeJson(res, .ok, .{
        .status = "ok",
        .service = "zombied",
        .database = "up",
        .commit = build_options.git_commit,
    });
}

pub fn handleReadyz(ctx: *Context, req: *httpz.Request, res: *httpz.Response) void {
    _ = req;
    const db_ok = databaseHealthy(ctx);
    const queue_dependency_ok = queueDependencyHealthy(ctx);

    if (!readyDecision(.{
        .db_ok = db_ok,
        .queue_dependency_ok = queue_dependency_ok,
    })) {
        common.writeJson(res, .service_unavailable, .{
            .ready = false,
            .database = db_ok,
            .queue_dependency = queue_dependency_ok,
        });
        return;
    }

    common.writeJson(res, .ok, .{
        .ready = true,
        .database = true,
        .queue_dependency = true,
    });
}

pub fn handleMetrics(ctx: *Context, req: *httpz.Request, res: *httpz.Response) void {
    _ = ctx;
    const body = metrics.renderPrometheus(
        req.arena,
        true,
    ) catch {
        res.status = @intFromEnum(std.http.Status.internal_server_error);
        res.body = "";
        return;
    };

    res.status = @intFromEnum(std.http.Status.ok);
    res.header("content-type", "text/plain; charset=utf-8");
    res.body = body;
}

test "integration: ready decision fails closed when redis queue dependency is degraded" {
    try std.testing.expect(!readyDecision(.{
        .db_ok = true,
        .queue_dependency_ok = false,
    }));
}

test "integration: ready decision passes when dependencies are healthy" {
    try std.testing.expect(readyDecision(.{
        .db_ok = true,
        .queue_dependency_ok = true,
    }));
}
