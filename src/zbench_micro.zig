// M25_001: zbench-backed micro-benchmark runner (Tier-1).
//
// HTTP loadgen (Tier-2) is handled externally by `hey` — see make/test-bench.mk.
// Each bench_xxx fn exercises one hot path; fixtures live in zbench_fixtures.zig.
// See docs/v2/active/P2_OBS_M25_001_ZBENCH_MICRO_CATALOG.md for gate rationale.

const std = @import("std");
const zbench = @import("zbench");

const router = @import("http/router.zig");
const error_registry = @import("errors/error_registry.zig");
const activity_cursor = @import("zombie/activity_cursor.zig");
const id_format = @import("types/id_format.zig");
const webhook_verify = @import("zombie/webhook_verify.zig");
const fx = @import("zbench_fixtures.zig");

// ── 1.1 route_match ───────────────────────────────────────────────────────
// Gate: p99 < 2 µs per call. Sweeps every Route arm once per iteration.
fn benchRouteMatch(allocator: std.mem.Allocator) void {
    _ = allocator;
    for (fx.ROUTE_PATHS) |path| {
        const r = router.match(path);
        std.mem.doNotOptimizeAway(r);
    }
}

// ── 1.2 error_registry_lookup ─────────────────────────────────────────────
// Gate: p99 < 100 ns per lookup. StaticStringMap hit + miss paths.
fn benchErrorRegistryLookup(allocator: std.mem.Allocator) void {
    _ = allocator;
    for (fx.ERROR_CODES) |code| {
        const entry = error_registry.lookup(code);
        std.mem.doNotOptimizeAway(entry);
    }
}

// ── 1.3 activity_cursor_roundtrip ─────────────────────────────────────────
// Gate: p99 < 1 µs per round-trip. parse → format → free.
fn benchActivityCursorRoundtrip(allocator: std.mem.Allocator) void {
    for (fx.CURSORS) |raw| {
        const parsed = activity_cursor.parse(raw) catch continue;
        const re = activity_cursor.format(allocator, parsed) catch continue;
        allocator.free(re);
    }
}

// ── 1.4 json_encode_response ──────────────────────────────────────────────
// Gate: p99 < 50 µs for the 10-zombie fixture.
fn benchJsonEncodeResponse(allocator: std.mem.Allocator) void {
    const body = .{ .zombies = fx.ZOMBIE_PAGE };
    const s = std.json.Stringify.valueAlloc(allocator, body, .{}) catch return;
    defer allocator.free(s);
    std.mem.doNotOptimizeAway(s.ptr);
}

// ── 1.6 uuid_v7_generate ──────────────────────────────────────────────────
// Gate: p99 < 2 µs per mint.
fn benchUuidV7Generate(allocator: std.mem.Allocator) void {
    const id = id_format.generateWorkspaceId(allocator) catch return;
    defer allocator.free(id);
    std.mem.doNotOptimizeAway(id.ptr);
}

// ── 1.7 webhook_signature_verify ──────────────────────────────────────────
// Gate: p99 < 10 µs per verify. GITHUB config — prefix + HMAC + constant-time eq.
fn benchWebhookSignatureVerify(allocator: std.mem.Allocator) void {
    _ = allocator;
    const ok = webhook_verify.verifySignature(
        webhook_verify.GITHUB,
        fx.WEBHOOK_SECRET,
        null,
        &fx.WEBHOOK_BODY,
        fx.WEBHOOK_SIGNATURE,
    );
    std.debug.assert(ok);
    std.mem.doNotOptimizeAway(ok);
}

// ── Entry point ───────────────────────────────────────────────────────────

pub fn main() !void {
    var gpa: std.heap.GeneralPurposeAllocator(.{}) = .{};
    defer _ = gpa.deinit();
    const alloc = gpa.allocator();

    var bench = zbench.Benchmark.init(alloc, .{
        .time_budget_ns = 200 * std.time.ns_per_ms, // 200 ms per benchmark
    });
    defer bench.deinit();

    try bench.add("route_match", benchRouteMatch, .{});
    try bench.add("error_registry_lookup", benchErrorRegistryLookup, .{});
    try bench.add("activity_cursor_roundtrip", benchActivityCursorRoundtrip, .{});
    try bench.add("json_encode_response", benchJsonEncodeResponse, .{});
    try bench.add("uuid_v7_generate", benchUuidV7Generate, .{});
    try bench.add("webhook_signature_verify", benchWebhookSignatureVerify, .{});

    const stdout: std.fs.File = .stdout();
    var buf: [4096]u8 = undefined;
    var writer = stdout.writer(&buf);
    try bench.run(&writer.interface);
    try writer.interface.flush();
}
