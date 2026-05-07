const std = @import("std");
const httpz = @import("httpz");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const metrics = @import("../../observability/metrics.zig");
const logging = @import("log");
const common = @import("common.zig");
const hx_mod = @import("hx.zig");
const build_options = @import("build_options");

pub const Context = common.Context;
const Hx = hx_mod.Hx;

const log = logging.scoped(.http);

// M10_001: QueueHealth struct and queueHealth() removed — they queried the
// dropped `runs` table for SPEC_QUEUED count. Zombie uses Redis streams,
// not DB-level queue depth. Queue depth/age metrics are no longer emitted.

const ReadyInputs = struct {
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
        log.warn("readyz.redis_check_failed", .{ .err = @errorName(err) });
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

// ── Response-shape tests (T1 + T7) ────────────────────────────────────────
// Pin the JSON envelope so a refactor cannot silently reintroduce removed
// fields or rename the renamed ones. These tests exercise innerHealthz
// directly via httpz.testing — the handler doesn't read hx.ctx, so ctx can
// be undefined.

fn buildLivenessHx(res: *httpz.Response) hx_mod.Hx {
    return .{
        .alloc = std.testing.allocator,
        .principal = undefined,
        .req_id = "health-test",
        .ctx = undefined, // innerHealthz does not read ctx
        .res = res,
    };
}

test "innerHealthz returns 200 with status/service/commit and NO database field" {
    // Regression guard — M18_002 dropped the `database` field from /healthz
    // (liveness does not probe DB; /readyz does that). If someone reintroduces
    // it, this test fails and forces the conversation.
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const hx = buildLivenessHx(ht.res);
    innerHealthz(hx);

    try ht.expectStatus(200);
    const json = try ht.getJson();
    const obj = json.object;
    try std.testing.expect(obj.get("status") != null);
    try std.testing.expect(obj.get("service") != null);
    try std.testing.expect(obj.get("commit") != null);
    // The anti-assertion: no database probe leaks into /healthz.
    try std.testing.expect(obj.get("database") == null);
    // Also guard the old/misspelled names that might resurface during merges.
    try std.testing.expect(obj.get("db") == null);
    try std.testing.expect(obj.get("queue_dependency") == null);
    try std.testing.expect(obj.get("queue") == null);
}

test "innerHealthz status value is the literal string \"ok\"" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const hx = buildLivenessHx(ht.res);
    innerHealthz(hx);

    const json = try ht.getJson();
    try std.testing.expectEqualStrings("ok", json.object.get("status").?.string);
    try std.testing.expectEqualStrings("zombied", json.object.get("service").?.string);
}
