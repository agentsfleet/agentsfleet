// XADD concurrency bench — N producer threads sharing one pool-backed Client.
//
// Skip-by-default unless `BENCH_REDIS=1`. `REDIS_URL` env overrides the
// default `redis://localhost:6379`. Output is one line of CSV-friendly
// stats plus per-thread durations.
//
// Drive via:
//   BENCH_REDIS=1 make bench-redis
// or:
//   BENCH_REDIS=1 zig build bench-redis -Dwith-bench-tools=true -Doptimize=ReleaseFast

const std = @import("std");
const app = @import("bench_app");
const queue = app.queue;
const Client = queue.Client;

const N_THREADS: usize = 8;
const N_OPS_PER_THREAD: usize = 1000;
const DEFAULT_REDIS_URL: []const u8 = "redis://localhost:6379";
const BENCH_ENV_GATE: []const u8 = "BENCH_REDIS";
const REDIS_URL_ENV: []const u8 = "REDIS_URL";
const STREAM_KEY_FMT: []const u8 = "bench:{d}:events";
const XADD_VERB: []const u8 = "XADD";
const AUTO_ID: []const u8 = "*";
const FIELD_NAME: []const u8 = "field";
const FIELD_VAL: []const u8 = "value";

const ThreadCtx = struct {
    tid: usize,
    client: *Client,
    elapsed_ns: u64 = 0,
    err: ?anyerror = null,
};

fn producerThread(ctx: *ThreadCtx) void {
    var key_buf: [64]u8 = undefined;
    const key = std.fmt.bufPrint(&key_buf, STREAM_KEY_FMT, .{ctx.tid}) catch |e| {
        ctx.err = e;
        return;
    };
    const t0 = std.time.nanoTimestamp();
    var i: usize = 0;
    while (i < N_OPS_PER_THREAD) : (i += 1) {
        var resp = ctx.client.command(&.{ XADD_VERB, key, AUTO_ID, FIELD_NAME, FIELD_VAL }) catch |e| {
            ctx.err = e;
            return;
        };
        resp.deinit(ctx.client.alloc);
    }
    ctx.elapsed_ns = @intCast(std.time.nanoTimestamp() - t0);
}

fn writeSummary(writer: anytype, total_ops: usize, elapsed_ns: u64, ctxs: []const ThreadCtx) !void {
    const elapsed_ms = @as(f64, @floatFromInt(elapsed_ns)) / 1_000_000.0;
    const ops_per_sec = @as(f64, @floatFromInt(total_ops)) * 1e9 / @as(f64, @floatFromInt(elapsed_ns));
    try writer.print("bench-redis xadd_concurrency: threads={d} ops_per_thread={d} total={d}\n", .{ N_THREADS, N_OPS_PER_THREAD, total_ops });
    try writer.print("  elapsed_ms={d:.2} ops_per_sec={d:.2}\n", .{ elapsed_ms, ops_per_sec });
    try writer.print("  per_thread_ms:", .{});
    for (ctxs) |c| try writer.print(" t{d}={d:.2}", .{ c.tid, @as(f64, @floatFromInt(c.elapsed_ns)) / 1_000_000.0 });
    try writer.print("\n", .{});
}

fn checkSkipGate(alloc: std.mem.Allocator, writer: anytype) !bool {
    const env = std.process.getEnvVarOwned(alloc, BENCH_ENV_GATE) catch |e| switch (e) {
        error.EnvironmentVariableNotFound => {
            try writer.print("bench-redis: skipped ({s} not set). Run with BENCH_REDIS=1 against a live Redis.\n", .{BENCH_ENV_GATE});
            return false;
        },
        else => return e,
    };
    defer alloc.free(env);
    if (env.len == 0 or std.mem.eql(u8, env, "0") or std.mem.eql(u8, env, "false")) {
        try writer.print("bench-redis: skipped ({s}={s}).\n", .{ BENCH_ENV_GATE, env });
        return false;
    }
    return true;
}

fn resolveRedisUrl(alloc: std.mem.Allocator) ![]u8 {
    return std.process.getEnvVarOwned(alloc, REDIS_URL_ENV) catch |e| switch (e) {
        error.EnvironmentVariableNotFound => try alloc.dupe(u8, DEFAULT_REDIS_URL),
        else => return e,
    };
}

pub fn main() !void {
    var gpa: std.heap.GeneralPurposeAllocator(.{}) = .{};
    defer _ = gpa.deinit();
    const alloc = gpa.allocator();

    const stdout: std.fs.File = .stdout();
    var out_buf: [4096]u8 = undefined;
    var stdout_writer = stdout.writer(&out_buf);
    const w = &stdout_writer.interface;
    defer w.flush() catch {};

    if (!try checkSkipGate(alloc, w)) return;

    const url = try resolveRedisUrl(alloc);
    defer alloc.free(url);
    try w.print("bench-redis: connecting to {s}\n", .{url});

    var client = try Client.connectFromUrl(alloc, url);
    defer client.deinit();

    var ctxs: [N_THREADS]ThreadCtx = undefined;
    for (&ctxs, 0..) |*c, i| c.* = .{ .tid = i, .client = &client };

    var threads: [N_THREADS]std.Thread = undefined;
    const t0 = std.time.nanoTimestamp();
    for (&threads, &ctxs) |*t, *c| t.* = try std.Thread.spawn(.{}, producerThread, .{c});
    for (threads) |t| t.join();
    const elapsed_ns: u64 = @intCast(std.time.nanoTimestamp() - t0);

    var failed: usize = 0;
    for (ctxs) |c| {
        if (c.err) |e| {
            try w.print("  thread t{d}: error {s}\n", .{ c.tid, @errorName(e) });
            failed += 1;
        }
    }
    if (failed > 0) {
        try w.print("bench-redis: {d}/{d} threads failed\n", .{ failed, N_THREADS });
        return error.BenchThreadFailed;
    }

    try writeSummary(w, N_THREADS * N_OPS_PER_THREAD, elapsed_ns, &ctxs);
}
