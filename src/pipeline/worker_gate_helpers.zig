//! Gate loop helper functions — extracted from worker_gate_loop.zig for the
//! 500-line limit (M21_002).

const std = @import("std");
const pg = @import("pg");
const queue_redis = @import("../queue/redis.zig");
const id_format = @import("../types/id_format.zig");
const codes = @import("../errors/codes.zig");
const gate_loop = @import("worker_gate_loop.zig");

const log = std.log.scoped(.gate_loop);

pub fn publishGateEvent(cfg: gate_loop.GateLoopConfig, result: gate_loop.GateToolResult, loop: u32) void {
    const redis_client = cfg.redis orelse return;
    const channel = std.fmt.allocPrint(cfg.alloc, "run:{s}:events", .{cfg.run_id}) catch return;
    defer cfg.alloc.free(channel);
    const outcome_str = if (result.passed) "PASS" else "FAIL";
    const created_at = std.time.milliTimestamp();
    const event_json = std.fmt.allocPrint(cfg.alloc,
        \\{{"gate_name":"{s}","outcome":"{s}","exit_code":{d},"loop":{d},"wall_ms":{d},"created_at":{d}}}
    , .{ result.gate_name, outcome_str, result.exit_code, loop, result.wall_ms, created_at }) catch return;
    defer cfg.alloc.free(event_json);
    redis_client.publish(channel, event_json) catch |err| {
        log.warn("gate_loop.pubsub_fail err={s} run_id={s}", .{ @errorName(err), cfg.run_id });
    };
}

pub fn persistGateResults(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    run_id: []const u8,
    results: []const gate_loop.GateToolResult,
) !void {
    const now_ms = std.time.milliTimestamp();
    for (results, 0..) |r, i| {
        const gate_id = try id_format.generateGateResultId(alloc);
        defer alloc.free(gate_id);
        _ = try conn.exec(
            "INSERT INTO gate_results (id, run_id, gate_name, attempt, exit_code, stdout_tail, stderr_tail, wall_ms, created_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
            .{ gate_id, run_id, r.gate_name, @as(i32, @intCast(i + 1)), @as(i32, @intCast(r.exit_code)), r.stdout, r.stderr, @as(i64, @intCast(r.wall_ms)), now_ms },
        );
    }
}

/// Format a markdown scorecard table from gate results.
pub fn formatScorecard(
    alloc: std.mem.Allocator,
    results: []const gate_loop.GateToolResult,
    total_repair_loops: u32,
    run_id: []const u8,
) ![]const u8 {
    var buf: std.ArrayList(u8) = .{};
    const w = buf.writer(alloc);
    try w.writeAll("## Gate Results\n\n");
    try w.writeAll("| Gate | Result | Wall Time |\n");
    try w.writeAll("|------|--------|-----------|\n");
    var total_wall: u64 = 0;
    for (results) |r| {
        total_wall += r.wall_ms;
        try w.print("| {s} | {s} | {d}ms |\n", .{
            r.gate_name,
            if (r.passed) "PASS" else "FAIL",
            r.wall_ms,
        });
    }
    try w.print("\n**Repair loops:** {d} | **Total wall time:** {d}ms | **Run ID:** {s}\n", .{
        total_repair_loops, total_wall, run_id,
    });
    return buf.toOwnedSlice(alloc);
}

pub fn buildRepairMessage(alloc: std.mem.Allocator, result: gate_loop.GateToolResult) ![]const u8 {
    return std.fmt.allocPrint(alloc,
        \\Gate '{s}' failed (exit code {d}).
        \\
        \\stdout:
        \\{s}
        \\
        \\stderr:
        \\{s}
        \\
        \\Fix the issue and re-run the gate.
    , .{ result.gate_name, result.exit_code, result.stdout, result.stderr });
}
