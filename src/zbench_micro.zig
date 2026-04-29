// M25_001: zbench-backed micro-benchmark runner (Tier-1).
//
// HTTP loadgen (Tier-2) is handled externally by `hey` — see make/test-bench.mk.
// Each bench_xxx fn exercises one hot path; fixtures live in zbench_fixtures.zig.
// See docs/v2/active/P2_OBS_M25_001_ZBENCH_MICRO_CATALOG.md for gate rationale.

const std = @import("std");
const zbench = @import("zbench");

const router = @import("http/router.zig");
const error_registry = @import("errors/error_registry.zig");
const keyset_cursor = @import("zombie/keyset_cursor.zig");
const id_format = @import("types/id_format.zig");
const webhook_verify = @import("zombie/webhook_verify.zig");
const pc = @import("executor/progress_callbacks.zig");
const fx = @import("zbench_fixtures.zig");

// Authoritative gates live in spec §1.X — these comments are pointers, not sources of truth.

// ── 1.1 route_match ─ spec §1.1
fn benchRouteMatch(allocator: std.mem.Allocator) void {
    _ = allocator;
    for (fx.ROUTE_PATHS) |path| {
        const r = router.match(path);
        std.mem.doNotOptimizeAway(r);
    }
}

// ── 1.2 error_registry_lookup ─ spec §1.2
fn benchErrorRegistryLookup(allocator: std.mem.Allocator) void {
    _ = allocator;
    for (fx.ERROR_CODES) |code| {
        const entry = error_registry.lookup(code);
        std.mem.doNotOptimizeAway(entry);
    }
}

// ── 1.3 keyset_cursor_roundtrip ─ spec §1.3
fn benchActivityCursorRoundtrip(allocator: std.mem.Allocator) void {
    for (fx.CURSORS) |raw| {
        // Fixtures are synthesized in-process and covered by keyset_cursor
        // unit tests; any failure here means the fixture builder drifted.
        const parsed = keyset_cursor.parse(raw) catch @panic("CURSORS fixture invalid");
        const re = keyset_cursor.format(allocator, parsed) catch @panic("cursor format OOM");
        allocator.free(re);
    }
}

// ── 1.4 json_encode_response ─ spec §1.4
fn benchJsonEncodeResponse(allocator: std.mem.Allocator) void {
    const body = .{ .zombies = fx.ZOMBIE_PAGE };
    const s = std.json.Stringify.valueAlloc(allocator, body, .{}) catch @panic("json encode OOM");
    defer allocator.free(s);
    std.mem.doNotOptimizeAway(s.ptr);
}

// ── 1.6 uuid_v7_generate ─ spec §1.6
fn benchUuidV7Generate(allocator: std.mem.Allocator) void {
    const id = id_format.generateWorkspaceId(allocator) catch @panic("uuid mint OOM");
    defer allocator.free(id);
    std.mem.doNotOptimizeAway(id.ptr);
}

// ── 1.7 webhook_signature_verify ─ spec §1.7
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
// Mirrors `activity_publisher.publishChunk` encode step exactly: shape
// the frame, serialize via valueAlloc, free. When the publisher migrates
// to a scratch ArrayList, this bench gets rewritten in lockstep so the
// pre/post delta surfaces cleanly.
fn benchActivityChunkEncode(allocator: std.mem.Allocator) void {
    const payload = std.json.Stringify.valueAlloc(allocator, .{
        .kind = "chunk",
        .event_id = fx.CHUNK_EVENT_ID,
        .text = fx.CHUNK_TEXT,
    }, .{}) catch @panic("chunk encode OOM");
    defer allocator.free(payload);
    std.mem.doNotOptimizeAway(payload.ptr);
}

// ── progress_frame_decode ─ executor → worker hot path
// Mirrors transport.sendRequestStreaming: parse once, discriminate
// progress vs terminal, decode the frame from the already-parsed value.
fn benchProgressFrameDecode(allocator: std.mem.Allocator) void {
    var parsed = std.json.parseFromSlice(std.json.Value, allocator, fx.PROGRESS_FRAME_BYTES, .{}) catch @panic("progress parse failed");
    defer parsed.deinit();
    const is_progress = pc.isProgressPayload(parsed.value);
    if (!is_progress) @panic("progress fixture invalid");
    const decoded = pc.decodeProgressFromValue(parsed.value) catch @panic("progress decode failed");
    std.mem.doNotOptimizeAway(&decoded.frame);
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
    try bench.add("keyset_cursor_roundtrip", benchActivityCursorRoundtrip, .{});
    try bench.add("json_encode_response", benchJsonEncodeResponse, .{});
    try bench.add("uuid_v7_generate", benchUuidV7Generate, .{});
    try bench.add("webhook_signature_verify", benchWebhookSignatureVerify, .{});
    try bench.add("activity_chunk_encode", benchActivityChunkEncode, .{});
    try bench.add("progress_frame_decode", benchProgressFrameDecode, .{});

    const stdout: std.fs.File = .stdout();
    var buf: [4096]u8 = undefined;
    var writer = stdout.writer(&buf);
    try bench.run(&writer.interface);
    try writer.interface.flush();
}
