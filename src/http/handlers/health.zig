const std = @import("std");
const httpz = @import("httpz");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const metrics = @import("../../observability/metrics.zig");
const obs_log = @import("../../observability/logging.zig");
const common = @import("common.zig");
const hx_mod = @import("hx.zig");
const build_options = @import("build_options");

pub const Context = common.Context;
const Hx = hx_mod.Hx;

// M10_001: QueueHealth struct and queueHealth() removed — they queried the
// dropped `runs` table for SPEC_QUEUED count. Zombie uses Redis streams,
// not DB-level queue depth. Queue depth/age metrics are no longer emitted.

pub const ReadyInputs = struct {
    db_ok: bool,
    queue_ok: bool,
};

fn databaseHealthy(ctx: *Context) bool {
    const conn = ctx.pool.acquire() catch return false;
    defer ctx.pool.release(conn);

    var ping = PgQuery.from(conn.query("SELECT 1", .{}) catch return false);
    defer ping.deinit();
    return (ping.next() catch null) != null;
}

fn queueHealthy(ctx: *Context) bool {
    ctx.queue.readyCheck() catch |err| {
        obs_log.logWarnErr(.http, err, "readyz: redis queue check failed", .{});
        return false;
    };
    return true;
}

pub fn readyDecision(inputs: ReadyInputs) bool {
    return inputs.db_ok and inputs.queue_ok;
}

pub fn innerHealthz(hx: Hx) void {
    // Liveness only — process is alive and able to serve an HTTP response.
    // Dependency checks (Postgres, Redis) live in /readyz; mixing them here
    // would flap liveness during transient dependency outages.
    hx.ok(.ok, .{
        .status = "ok",
        .service = "zombied",
        .commit = build_options.git_commit,
    });
}

pub fn innerReadyz(hx: Hx) void {
    const db_ok = databaseHealthy(hx.ctx);
    const queue_ok = queueHealthy(hx.ctx);

    if (!readyDecision(.{ .db_ok = db_ok, .queue_ok = queue_ok })) {
        hx.ok(.service_unavailable, .{
            .ready = false,
            .database = db_ok,
            .queue = queue_ok,
        });
        return;
    }

    hx.ok(.ok, .{
        .ready = true,
        .database = true,
        .queue = true,
    });
}

pub fn innerMetrics(hx: Hx, req: *httpz.Request) void {
    // Prometheus exposition is text/plain, not JSON — write res directly.
    const body = metrics.renderPrometheus(req.arena, true) catch {
        hx.res.status = @intFromEnum(std.http.Status.internal_server_error);
        hx.res.body = "";
        return;
    };
    hx.res.status = @intFromEnum(std.http.Status.ok);
    hx.res.header("content-type", "text/plain; charset=utf-8");
    hx.res.body = body;
}

test "integration: ready decision fails closed when redis queue dependency is degraded" {
    try std.testing.expect(!readyDecision(.{
        .db_ok = true,
        .queue_ok = false,
    }));
}

test "integration: ready decision passes when dependencies are healthy" {
    try std.testing.expect(readyDecision(.{
        .db_ok = true,
        .queue_ok = true,
    }));
}
