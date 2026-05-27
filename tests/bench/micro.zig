// Tier-1 micro-bench runner — zBench-backed.
//
// HTTP loadgen (Tier-2) is handled externally by `hey` — see make/bench.mk.
// Each bench_xxx fn exercises one hot path; fixtures live in micro_fixtures.zig.

const std = @import("std");
const zbench = @import("zbench");
const app = @import("bench_app");

const router = app.router;
const error_registry = app.error_registry;
const keyset_cursor = app.keyset_cursor;
const id_format = app.id_format;
const webhook_verify = app.webhook_verify;
const fx = @import("micro_fixtures.zig");

// ── route_match ───────────────────────────────────────────────────────────
fn benchRouteMatch(allocator: std.mem.Allocator) void {
    _ = allocator;
    for (fx.ROUTE_PATHS) |path| {
        const r = router.match(path, .GET);
        std.mem.doNotOptimizeAway(r);
    }
}

// ── error_registry_lookup ─────────────────────────────────────────────────
fn benchErrorRegistryLookup(allocator: std.mem.Allocator) void {
    _ = allocator;
    for (fx.ERROR_CODES) |code| {
        const entry = error_registry.lookup(code);
        std.mem.doNotOptimizeAway(entry);
    }
}

// ── keyset_cursor_roundtrip ───────────────────────────────────────────────
fn benchActivityCursorRoundtrip(allocator: std.mem.Allocator) void {
    for (fx.CURSORS) |raw| {
        // Fixtures are synthesized in-process and covered by keyset_cursor
        // unit tests; any failure here means the fixture builder drifted.
        const parsed = keyset_cursor.parse(raw) catch @panic("CURSORS fixture invalid");
        const re = keyset_cursor.format(allocator, parsed) catch @panic("cursor format OOM");
        allocator.free(re);
    }
}

// ── json_encode_response ──────────────────────────────────────────────────
fn benchJsonEncodeResponse(allocator: std.mem.Allocator) void {
    const body = .{ .zombies = fx.ZOMBIE_PAGE };
    const s = std.json.Stringify.valueAlloc(allocator, body, .{}) catch @panic("json encode OOM");
    defer allocator.free(s);
    std.mem.doNotOptimizeAway(s.ptr);
}

// ── uuid_v7_generate ──────────────────────────────────────────────────────
fn benchUuidV7Generate(allocator: std.mem.Allocator) void {
    const id = id_format.generateWorkspaceId(allocator) catch @panic("uuid mint OOM");
    defer allocator.free(id);
    std.mem.doNotOptimizeAway(id.ptr);
}

// ── webhook_signature_verify ──────────────────────────────────────────────
fn benchWebhookSignatureVerify(allocator: std.mem.Allocator) void {
    _ = allocator;
    const ok = webhook_verify.verifySignature(
        webhook_verify.GITHUB,
        fx.WEBHOOK_SECRET,
        null,
        &fx.WEBHOOK_BODY,
        fx.WEBHOOK_SIGNATURE,
    );
    // @panic survives ReleaseFast — std.debug.assert would be elided and
    // silently measure the reject path if the comptime fixture ever drifted.
    if (!ok) @panic("webhook fixture invalid");
    std.mem.doNotOptimizeAway(ok);
}

// ── activity_chunk_encode ─ streaming-substrate hot path
// Mirrors `activity_publisher.publishChunk` encode step: clearRetaining
// the per-event scratch buffer, encode the frame via the Writer
// interface. Steady-state allocator round-trips → 0 after warmup.
//
// Process-lifetime scratch — initialized in main() under the bench
// allocator and torn down explicitly before main() returns. zbench
// drives this fn with the same allocator across iterations.
// SAFETY: test fixture; field is populated by the surrounding builder before any read.
var bench_chunk_scratch: std.io.Writer.Allocating = undefined;

fn benchActivityChunkEncode(allocator: std.mem.Allocator) void {
    _ = allocator;
    bench_chunk_scratch.clearRetainingCapacity();
    std.json.Stringify.value(.{
        .kind = "chunk",
        .event_id = fx.CHUNK_EVENT_ID,
        .text = fx.CHUNK_TEXT,
    }, .{}, &bench_chunk_scratch.writer) catch @panic("chunk encode failed");
    std.mem.doNotOptimizeAway(bench_chunk_scratch.written().ptr);
}

// The progress_frame_decode bench mirrored the executor→worker transport
// decode, removed at the M80 cutover when execution moved to the runner.

// ── Entry point ───────────────────────────────────────────────────────────

pub fn main() !void {
    var gpa: std.heap.GeneralPurposeAllocator(.{}) = .{};
    defer _ = gpa.deinit();
    const alloc = gpa.allocator();

    var bench = zbench.Benchmark.init(alloc, .{
        .time_budget_ns = 200 * std.time.ns_per_ms, // 200 ms per benchmark
    });
    defer bench.deinit();

    bench_chunk_scratch = .init(alloc);
    defer bench_chunk_scratch.deinit();

    try bench.add("route_match", benchRouteMatch, .{});
    try bench.add("error_registry_lookup", benchErrorRegistryLookup, .{});
    try bench.add("keyset_cursor_roundtrip", benchActivityCursorRoundtrip, .{});
    try bench.add("json_encode_response", benchJsonEncodeResponse, .{});
    try bench.add("uuid_v7_generate", benchUuidV7Generate, .{});
    try bench.add("webhook_signature_verify", benchWebhookSignatureVerify, .{});
    try bench.add("activity_chunk_encode", benchActivityChunkEncode, .{});

    const stdout: std.fs.File = .stdout();
    var buf: [4096]u8 = undefined;
    var writer = stdout.writer(&buf);
    try bench.run(&writer.interface);
    try writer.interface.flush();
}
