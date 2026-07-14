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
const credential_broker = app.credential_broker;
const credential_integration = app.credential_integration;
const ZeroizingAllocator = app.ZeroizingAllocator;
const fx = @import("micro_fixtures.zig");

const ZEROING_BENCH_BYTES: usize = 4 * 1024;

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
    const body = .{ .agents = fx.AGENTSFLEET_PAGE };
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

// ── activity_chunk_encode ─ streaming-substrate hot path
// Mirrors `activity_publisher.publishChunk` encode step: clearRetaining
// the per-event scratch buffer, encode the frame via the Writer
// interface. Steady-state allocator round-trips → 0 after warmup.
//
// Process-lifetime scratch — initialized in main() under the bench
// allocator and torn down explicitly before main() returns. zbench
// drives this fn with the same allocator across iterations.
// SAFETY: test fixture; field is populated by the surrounding builder before any read.
var bench_chunk_scratch: std.Io.Writer.Allocating = undefined;

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

// The progress_frame_decode bench mirrored the pre-cutover in-process transport
// decode, removed at the M80 cutover when execution moved to the runner.

// ── credential_broker_cache_hit ─ the mint hot path
// A token is minted once and then served from cache on every tool call for the
// lease's life, so the CACHE HIT is the hot path. This measures the lock-free
// read (cache.zig SHARED RwLock) + the caller dup + the metrics emit. A
// regression that put an exclusive lock back on this read would show here.
//
// Process-lifetime broker + handle — built in main() under the bench allocator,
// warmed once so every iteration is a hit, torn down before main() returns.
// Single-sourced by the warm-up + the cache-hit bench (RULE UFS); the integration
// id ties to the enum so a rename can't drift the bench.
const BENCH_WS = "ws-bench";
const BENCH_STATIC_ID = @tagName(credential_integration.Id.static);

// SAFETY: populated in main() before the bench runs; torn down after.
var bench_broker: credential_broker = undefined;
// SAFETY: populated in main() before the bench runs; torn down after.
var bench_handle: std.json.Parsed(std.json.Value) = undefined;

fn benchBrokerCacheHit(allocator: std.mem.Allocator) void {
    const r = bench_broker.mint(allocator, BENCH_WS, BENCH_STATIC_ID, bench_handle.value, 0) catch @panic("broker mint failed");
    if (r != .ok) @panic("broker cache-hit not ok");
    allocator.free(r.ok.token);
}

// ── credential_static_mint_dispatch ─ the per-integration dispatch cost
// Isolates the registry resolve + tagged-union strategy dispatch + token dup,
// with no cache and no network (the `static` integration). The cold-path compute
// floor a real mint pays on top of the GitHub round-trip.
fn benchStaticMintDispatch(allocator: std.mem.Allocator) void {
    const spec = credential_integration.resolve(credential_integration.REGISTRY, .static).?;
    const nd = credential_integration.nullDeps();
    const out = spec.mint.run(.{
        .alloc = allocator,
        .handle = bench_handle.value,
        .now_ms = 0,
        .platform = .{},
        .http = nd.http,
        .sign = nd.sign,
    }) catch @panic("static mint dispatch failed");
    if (out != .ok) @panic("static mint dispatch not ok");
    allocator.free(out.ok.token);
}

// ── zeroizing_free_4k ─ request-arena teardown floor
fn benchZeroizingFree(allocator: std.mem.Allocator) void {
    var zeroing = ZeroizingAllocator.wrap(allocator);
    const alloc = zeroing.allocator();
    const secret = alloc.alloc(u8, ZEROING_BENCH_BYTES) catch @panic("zeroing allocation failed");
    @memset(secret, 0xA5);
    alloc.free(secret);
}

// ── Entry point ───────────────────────────────────────────────────────────

pub fn main() !void {
    var gpa: std.heap.DebugAllocator(.{}) = .init;
    defer _ = gpa.deinit();
    const alloc = gpa.allocator();

    var bench = zbench.Benchmark.init(alloc, .{
        .time_budget_ns = 200 * std.time.ns_per_ms, // 200 ms per benchmark
    });
    defer bench.deinit();

    bench_chunk_scratch = .init(alloc);
    defer bench_chunk_scratch.deinit();

    bench_broker = try credential_broker.init(alloc, credential_integration.REGISTRY, credential_integration.nullDeps());
    defer bench_broker.deinit();
    bench_handle = try std.json.parseFromSlice(std.json.Value, alloc, "{\"integration\":\"static\",\"token\":\"ghs_bench_token\"}", .{});
    defer bench_handle.deinit();
    // Warm the cache once so broker_cache_hit measures the HIT (the hot path).
    {
        const warm = try bench_broker.mint(alloc, BENCH_WS, BENCH_STATIC_ID, bench_handle.value, 0);
        if (warm == .ok) alloc.free(warm.ok.token);
    }

    try bench.add("route_match", benchRouteMatch, .{});
    try bench.add("error_registry_lookup", benchErrorRegistryLookup, .{});
    try bench.add("keyset_cursor_roundtrip", benchActivityCursorRoundtrip, .{});
    try bench.add("json_encode_response", benchJsonEncodeResponse, .{});
    try bench.add("uuid_v7_generate", benchUuidV7Generate, .{});
    try bench.add("activity_chunk_encode", benchActivityChunkEncode, .{});
    try bench.add("broker_cache_hit", benchBrokerCacheHit, .{});
    try bench.add("static_mint_dispatch", benchStaticMintDispatch, .{});
    try bench.add("zeroizing_free_4k", benchZeroizingFree, .{});

    // zbench 0.11.2's run writes + flushes to the File directly on the 0.16 io.
    try bench.run(app.globalIo(), std.Io.File.stdout());
}
