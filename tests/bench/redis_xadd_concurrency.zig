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
const clock = app.clock;

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
    const t0 = clock.nowNanos();
    var i: usize = 0;
    while (i < N_OPS_PER_THREAD) : (i += 1) {
        var resp = ctx.client.command(&.{ XADD_VERB, key, AUTO_ID, FIELD_NAME, FIELD_VAL }) catch |e| {
            ctx.err = e;
            return;
        };
        resp.deinit(ctx.client.alloc);
    }
    ctx.elapsed_ns = @intCast(clock.nowNanos() - t0);
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

fn checkSkipGate(env_map: *const app.env.Map, writer: anytype) !bool {
    const gate = env_map.get(BENCH_ENV_GATE) orelse {
        try writer.print("bench-redis: skipped ({s} not set). Run with BENCH_REDIS=1 against a live Redis.\n", .{BENCH_ENV_GATE});
        return false;
    };
    if (gate.len == 0 or std.mem.eql(u8, gate, "0") or std.mem.eql(u8, gate, "false")) {
        try writer.print("bench-redis: skipped ({s}={s}).\n", .{ BENCH_ENV_GATE, gate });
        return false;
    }
    return true;
}

// Borrowed from the live-environ snapshot owned by `main`; no allocation.
fn resolveRedisUrl(env_map: *const app.env.Map) []const u8 {
    return env_map.get(REDIS_URL_ENV) orelse DEFAULT_REDIS_URL;
}

pub fn main() !void {
    var gpa: std.heap.DebugAllocator(.{}) = .init;
    defer _ = gpa.deinit();
    const alloc = gpa.allocator();

    const stdout: std.Io.File = .stdout();
    var out_buf: [4096]u8 = undefined;
    var stdout_writer = stdout.writer(app.globalIo(), &out_buf);
    const w = &stdout_writer.interface;
    // Best-effort flush at scope exit — bench tool, terminal write failures
    // are not actionable here.
    defer if (w.flush()) |_| {} else |_| {};

    // Zig 0.16 removed getEnvVarOwned; snapshot the live environ once. Borrowed
    // gate/url slices stay valid as long as the snapshot (outlives the client).
    var env_map = try app.env.testLiveSnapshot(alloc);
    defer env_map.deinit();

    if (!try checkSkipGate(&env_map, w)) return;

    const url = resolveRedisUrl(&env_map);
    try w.print("bench-redis: connecting to {s}\n", .{url});

    // `queue.testing.connectFromUrl` resolves the CA from REDIS_TLS_CA_CERT_FILE
    // so the bench can reach a self-signed dev Redis; falls back to no-CA when
    // the env is unset (real-cert / plain Redis). Same pool/XADD path measured.
    var client = try queue.testing.connectFromUrl(app.globalIo(), alloc, url);
    defer client.deinit();

    var ctxs: [N_THREADS]ThreadCtx = undefined;
    for (&ctxs, 0..) |*c, i| c.* = .{ .tid = i, .client = &client };

    var threads: [N_THREADS]std.Thread = undefined;
    const t0 = clock.nowNanos();
    for (&threads, &ctxs) |*t, *c| t.* = try std.Thread.spawn(.{}, producerThread, .{c});
    for (threads) |t| t.join();
    const elapsed_ns: u64 = @intCast(clock.nowNanos() - t0);

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
