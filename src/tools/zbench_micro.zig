// M24_001: zbench-backed micro-benchmark runner (Tier-1).
//
// HTTP loadgen (Tier-2) is handled externally by `hey` — see make/test-bench.mk.
// This executable runs code-level micro-benchmarks. Invoked by `make bench`.
//
// Today: one no-op placeholder benchmark. The catalog of real micro-benchmarks
// to add is specified in docs/v2/pending/P2_OBS_M25_001_ZBENCH_MICRO_CATALOG.md.
// Contributors: add a new `bench_xxx` fn below and register it in `main`.

const std = @import("std");
const zbench = @import("zbench");

// ── Benchmarks ────────────────────────────────────────────────────────────────

fn benchNoop(allocator: std.mem.Allocator) void {
    _ = allocator;
    // Placeholder. Replace with real code-level micro-benchmarks per M25_001.
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn main() !void {
    var gpa: std.heap.GeneralPurposeAllocator(.{}) = .{};
    defer _ = gpa.deinit();
    const alloc = gpa.allocator();

    var bench = zbench.Benchmark.init(alloc, .{
        .time_budget_ns = 200 * std.time.ns_per_ms, // 200ms per benchmark; keeps CI fast.
    });
    defer bench.deinit();

    try bench.add("noop", benchNoop, .{});

    const stdout: std.fs.File = .stdout();
    var buf: [4096]u8 = undefined;
    var writer = stdout.writer(&buf);
    try bench.run(&writer.interface);
    try writer.interface.flush();
}
