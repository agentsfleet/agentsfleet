// Integration tests for the runner-plane memory loop's loss counters — drives
// the full HTTP verbs through the in-process TestHarness (runner bearer auth,
// seeded fleet rows, live test DB):
//   GET  /v1/runners/me/memory/{fleet_id} — hydration-window drop counters
//   POST /v1/runners/me/memory/{fleet_id} — cap-eviction / truncation / skip
//
// The harness server runs in-process, so the metrics globals asserted here are
// the same atomics the handlers increment; before/after snapshots give exact
// deltas (Zig tests in one binary run sequentially, so no cross-test races).
// Sibling memory_fencing_test.zig covers the lease/fencing authorization at
// the resolver tier; this suite stays on the happy-auth path and asserts loss
// accounting. Requires LIVE_DB; skips when TEST_DATABASE_URL is unset.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const base = @import("../../../db/test_fixtures.zig");
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const auth_mw = @import("../../../auth/middleware/mod.zig");
const serve_runner_lookup = @import("../../../cmd/serve_runner_lookup.zig");
const api_key = @import("../../../auth/api_key.zig");
const metrics_memory = @import("../../../observability/metrics_memory.zig");
const memory_adapter = @import("../../../memory/fleet_memory.zig");
const protocol = @import("contract").protocol;
const clock = @import("common").clock;

const ALLOC = std.testing.allocator;

// Distinct UUIDv7 literals (no collision with sibling fleet/memory tests).
const WORKSPACE_ID = "0195e2aa-4c1b-7f13-8abc-2b3e1e0f1011";
const RUNNER_ID = "0195e2aa-4c1b-7f13-8abc-2b3e1e0f1a01";
const ZID_HYD_DROP = "0195e2aa-4c1b-7f13-8abc-2b3e1e0f1c01";
const ZID_HYD_FIT = "0195e2aa-4c1b-7f13-8abc-2b3e1e0f1c02";
const ZID_CAP_OVER = "0195e2aa-4c1b-7f13-8abc-2b3e1e0f1c03";
const ZID_CAP_UNDER = "0195e2aa-4c1b-7f13-8abc-2b3e1e0f1c04";
const ZID_TRUNC = "0195e2aa-4c1b-7f13-8abc-2b3e1e0f1c05";
const ZID_SKIP = "0195e2aa-4c1b-7f13-8abc-2b3e1e0f1c06";
const ZID_HYD_CORE = "0195e2aa-4c1b-7f13-8abc-2b3e1e0f1c07";
const ZID_SWEEP = "0195e2aa-4c1b-7f13-8abc-2b3e1e0f1c08";
const ZID_SWEEP_CAP = "0195e2aa-4c1b-7f13-8abc-2b3e1e0f1c09";
const LEASE_HYD_DROP = "0195e2aa-4c1b-7f13-8abc-2b3e1e0f1f01";
const LEASE_HYD_FIT = "0195e2aa-4c1b-7f13-8abc-2b3e1e0f1f02";
const LEASE_CAP_OVER = "0195e2aa-4c1b-7f13-8abc-2b3e1e0f1f03";
const LEASE_CAP_UNDER = "0195e2aa-4c1b-7f13-8abc-2b3e1e0f1f04";
const LEASE_TRUNC = "0195e2aa-4c1b-7f13-8abc-2b3e1e0f1f05";
const LEASE_SKIP = "0195e2aa-4c1b-7f13-8abc-2b3e1e0f1f06";
const LEASE_HYD_CORE = "0195e2aa-4c1b-7f13-8abc-2b3e1e0f1f07";
const LEASE_SWEEP = "0195e2aa-4c1b-7f13-8abc-2b3e1e0f1f08";
const LEASE_SWEEP_CAP = "0195e2aa-4c1b-7f13-8abc-2b3e1e0f1f09";
const EVENT_ID = "evt-mem-loop-1";
const NOW_MS: i64 = 1_900_000_000_000;
/// Seeded lease fencing token; every push fences at exactly this value.
const FENCE: u64 = 5;

const RUNNER_TOKEN = auth_mw.runner_bearer.RUNNER_TOKEN_PREFIX ++ "f" ** 64;

/// Per-row hydrate fixture sizing: key "hk{n}" (3) + content (60_000) +
/// category "c" (1). Five rows at 60_004 bytes against the 256 KiB window
/// keep the newest four and drop exactly one (the Compactor admits row 0
/// unconditionally, then cuts where the running total would cross budget).
const HYD_CONTENT_LEN: usize = 60_000;
const HYD_ROW_BYTES: usize = 3 + HYD_CONTENT_LEN + 1;
const HYD_ROWS: usize = 5;

/// Truncation fixture sizing: key "tk{nn}" (4) + content (16 KiB, the
/// per-delta MAX_CONTENT_LEN cap) + category "c" (1). The capture loop stops
/// once the running byte total would cross MAX_MEMORY_PUSH_BYTES, so the kept
/// prefix is the floor division below.
const TRUNC_CONTENT = "x" ** (16 * 1024);
const TRUNC_DELTA_BYTES: usize = 4 + TRUNC_CONTENT.len + 1;
const TRUNC_DELTAS: usize = protocol.MAX_MEMORY_PUSH_BYTES / TRUNC_DELTA_BYTES + 2;
const TRUNC_KEPT: usize = protocol.MAX_MEMORY_PUSH_BYTES / TRUNC_DELTA_BYTES;

// SAFETY: populated by configureRegistry before the middleware chain reads it.
var runner_lookup_ctx: serve_runner_lookup.Ctx = undefined;

fn configureRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    runner_lookup_ctx = .{ .pool = h.pool };
    reg.runner_bearer_mw = .{ .host = &runner_lookup_ctx, .lookup = serve_runner_lookup.lookup };
}

fn seedRunner(conn: *pg.Conn) !void {
    const hash = api_key.sha256Hex(RUNNER_TOKEN);
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, 'mem-loop-host', $2, 'dev_none', 'active', '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ RUNNER_ID, hash[0..] });
}

/// Seed an active, unexpired lease binding the runner to `fleet_id` at FENCE.
fn seedLease(conn: *pg.Conn, lease_id: []const u8, fleet_id: []const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_leases
        \\  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, actor,
        \\   event_type, request_json, event_created_at, posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   fencing_token, lease_expires_at, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6, 'steer:test',
        \\        'chat', '{"message":"hi"}', 0, 'platform', 'p', 'm', 0, 0, 0, 0,
        \\        $7, $8, 'active', 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ lease_id, RUNNER_ID, fleet_id, WORKSPACE_ID, base.TEST_TENANT_ID, EVENT_ID, @as(i64, FENCE), NOW_MS + 30_000 });
}

fn execIgnore(conn: *pg.Conn, sql: []const u8, args: anytype) void {
    _ = conn.exec(sql, args) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

fn wipeMemory(conn: *pg.Conn, fleet_id: []const u8) void {
    execIgnore(conn, "SET ROLE memory_runtime", .{});
    execIgnore(conn, "DELETE FROM memory.memory_entries WHERE fleet_id = $1::uuid", .{fleet_id});
    execIgnore(conn, "RESET ROLE", .{});
}

fn teardown(conn: *pg.Conn) void {
    inline for (.{ ZID_HYD_DROP, ZID_HYD_FIT, ZID_CAP_OVER, ZID_CAP_UNDER, ZID_TRUNC, ZID_SKIP, ZID_HYD_CORE, ZID_SWEEP, ZID_SWEEP_CAP }) |zid| {
        wipeMemory(conn, zid);
    }
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE runner_id = $1::uuid", .{RUNNER_ID});
    execIgnore(conn, "DELETE FROM fleet.runners WHERE id = $1::uuid", .{RUNNER_ID});
    base.teardownWorkspace(conn, WORKSPACE_ID);
}

const Env = struct {
    h: *TestHarness,

    fn deinit(self: *Env) void {
        if (self.h.acquireConn()) |conn| {
            defer self.h.releaseConn(conn);
            teardown(conn);
        } else |_| {}
        self.h.deinit();
    }
};

/// Start the harness, seed tenant/workspace/runner + every per-fleet lease.
fn setup() !?Env {
    const h = TestHarness.start(ALLOC, .{ .configureRegistry = configureRegistry }) catch |err| {
        if (err == error.SkipZigTest) return null;
        return err;
    };
    errdefer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    teardown(conn);
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try seedRunner(conn);
    try seedLease(conn, LEASE_HYD_DROP, ZID_HYD_DROP);
    try seedLease(conn, LEASE_HYD_FIT, ZID_HYD_FIT);
    try seedLease(conn, LEASE_CAP_OVER, ZID_CAP_OVER);
    try seedLease(conn, LEASE_CAP_UNDER, ZID_CAP_UNDER);
    try seedLease(conn, LEASE_TRUNC, ZID_TRUNC);
    try seedLease(conn, LEASE_SKIP, ZID_SKIP);
    try seedLease(conn, LEASE_HYD_CORE, ZID_HYD_CORE);
    try seedLease(conn, LEASE_SWEEP, ZID_SWEEP);
    try seedLease(conn, LEASE_SWEEP_CAP, ZID_SWEEP_CAP);
    return .{ .h = h };
}

fn memoryUrl(fleet_id: []const u8) ![]u8 {
    return std.fmt.allocPrint(ALLOC, "/v1/runners/me/memory/{s}", .{fleet_id});
}

/// Seed `n` durable rows for `fleet_id` directly (memory_runtime INSERT),
/// content `repeat('x', content_len)`, updated_at ascending from a fixed cold
/// epoch so any subsequently pushed entry is strictly newer.
fn seedRows(env: Env, fleet_id: []const u8, n: usize, content_len: usize) !void {
    const conn = try env.h.acquireConn();
    defer env.h.releaseConn(conn);
    _ = try conn.exec("SET ROLE memory_runtime", .{});
    defer execIgnore(conn, "RESET ROLE", .{});
    // uid must satisfy the UUIDv7 check (version nibble '7'); compose a
    // deterministic v7-shaped uid from the fleet's distinguishing tail
    // (dashless chars 25..32) + n, so uids never collide across the six
    // fixture fleets. The id column is globally UNIQUE, so it carries the
    // same tail; the key column stays per-fleet ('hk' || n) because the
    // hydrate byte arithmetic depends on its exact length.
    _ = try conn.exec(
        \\INSERT INTO memory.memory_entries
        \\  (uid, id, key, content, category, fleet_id, created_at, updated_at)
        \\SELECT (('0195e2aa-4c1b-7' || lpad(to_hex(n), 3, '0') || '-8abc-'
        \\         || substr(replace($1::uuid::text, '-', ''), 25, 8) || lpad(to_hex(n), 4, '0')))::uuid,
        \\       'hk-' || substr(replace($1::uuid::text, '-', ''), 29, 4) || '-' || n,
        \\       'hk' || n, repeat('x', $2::int), 'c', $1::uuid,
        \\       1700000000000, 1700000000000 + n
        \\FROM generate_series(1, $3::int) n
        \\ON CONFLICT (key, fleet_id) DO NOTHING
    , .{ fleet_id, @as(i64, @intCast(content_len)), @as(i64, @intCast(n)) });
}

/// Seed one cold core fact directly (memory_runtime INSERT) — updated_at far
/// below seedRows' epoch so every windowed row is strictly newer than it.
fn seedCoreRow(env: Env, fleet_id: []const u8, key: []const u8) !void {
    const conn = try env.h.acquireConn();
    defer env.h.releaseConn(conn);
    _ = try conn.exec("SET ROLE memory_runtime", .{});
    defer execIgnore(conn, "RESET ROLE", .{});
    _ = try conn.exec(
        \\INSERT INTO memory.memory_entries
        \\  (uid, id, key, content, category, fleet_id, created_at, updated_at)
        \\VALUES (('0195e2aa-4c1b-7fff-8abc-' || substr(replace($1::uuid::text, '-', ''), 25, 8) || 'ffff')::uuid,
        \\        'ck-' || substr(replace($1::uuid::text, '-', ''), 29, 4),
        \\        $2, 'indy', $3, $1::uuid, 1600000000000, 1600000000000)
        \\ON CONFLICT (key, fleet_id) DO NOTHING
    , .{ fleet_id, key, memory_adapter.CATEGORY_CORE });
}

/// Seed one `daily` row at an explicit `updated_at` (epoch ms) — the retention
/// sweep's age fixture. `n` (1..15) keys the uid lane ('7dd' + hex nibble), so
/// rows never collide with seedRows ('7' + 3-hex) or seedCoreRow ('7fff').
fn seedDailyAt(env: Env, fleet_id: []const u8, key: []const u8, n: u8, updated_at_ms: i64) !void {
    const conn = try env.h.acquireConn();
    defer env.h.releaseConn(conn);
    _ = try conn.exec("SET ROLE memory_runtime", .{});
    defer execIgnore(conn, "RESET ROLE", .{});
    _ = try conn.exec(
        \\INSERT INTO memory.memory_entries
        \\  (uid, id, key, content, category, fleet_id, created_at, updated_at)
        \\VALUES (('0195e2aa-4c1b-7dd' || to_hex($5::int) || '-8abc-'
        \\         || substr(replace($1::uuid::text, '-', ''), 25, 8) || '0' || to_hex($5::int) || '00')::uuid,
        \\        'sk-' || substr(replace($1::uuid::text, '-', ''), 29, 4) || '-' || $5::int,
        \\        $2, 'scratch', $3, $1::uuid, $4, $4)
        \\ON CONFLICT (key, fleet_id) DO NOTHING
    , .{ fleet_id, key, memory_adapter.CATEGORY_DAILY, updated_at_ms, @as(i32, n) });
}

fn rowCount(env: Env, fleet_id: []const u8) !i64 {
    const conn = try env.h.acquireConn();
    defer env.h.releaseConn(conn);
    _ = try conn.exec("SET ROLE memory_runtime", .{});
    defer execIgnore(conn, "RESET ROLE", .{});
    var q = PgQuery.from(try conn.query("SELECT COUNT(*) FROM memory.memory_entries WHERE fleet_id = $1::uuid", .{fleet_id}));
    defer q.deinit();
    const row = try q.next() orelse return 0;
    return try row.get(i64, 0);
}

/// Build a MemoryPushRequest body for `lease_id` with `count` deltas keyed
/// "{prefix}{index:0>2}" and a shared content/category. Caller frees.
fn buildPush(lease_id: []const u8, prefix: []const u8, count: usize, content: []const u8) ![]u8 {
    var aw: std.Io.Writer.Allocating = .init(ALLOC);
    errdefer aw.deinit();
    try aw.writer.print("{{\"lease_id\":\"{s}\",\"fencing_token\":{d},\"memory\":[", .{ lease_id, FENCE });
    var i: usize = 0;
    while (i < count) : (i += 1) {
        if (i > 0) try aw.writer.writeAll(",");
        try aw.writer.print("{{\"key\":\"{s}{d:0>2}\",\"content\":\"{s}\",\"category\":\"c\"}}", .{ prefix, i + 1, content });
    }
    try aw.writer.writeAll("]}");
    return aw.toOwnedSlice();
}

// ── §hydrate: window-drop counters ──────────────────────────────────────────

test "test_hydrate_drop_counters_exact" {
    var env = (try setup()) orelse return error.SkipZigTest;
    defer env.deinit();
    try seedRows(env, ZID_HYD_DROP, HYD_ROWS, HYD_CONTENT_LEN);

    const url = try memoryUrl(ZID_HYD_DROP);
    defer ALLOC.free(url);

    const before = metrics_memory.snapshot();
    const r = try (try env.h.get(url).bearer(RUNNER_TOKEN)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    const after = metrics_memory.snapshot();
    // Five 60_004-byte rows against the 256 KiB window: four kept, one dropped
    // — the counters move by the exact entry/byte arithmetic of the window.
    try std.testing.expectEqual(before.hydration_dropped_entries_total + 1, after.hydration_dropped_entries_total);
    try std.testing.expectEqual(before.hydration_dropped_bytes_total + HYD_ROW_BYTES, after.hydration_dropped_bytes_total);
    try std.testing.expectEqual(@as(i64, HYD_ROWS - 1), after.hydration_entries);
}

test "test_hydrate_no_drop_when_fits" {
    var env = (try setup()) orelse return error.SkipZigTest;
    defer env.deinit();
    try seedRows(env, ZID_HYD_FIT, 2, 64);

    const url = try memoryUrl(ZID_HYD_FIT);
    defer ALLOC.free(url);

    const before = metrics_memory.snapshot();
    const r = try (try env.h.get(url).bearer(RUNNER_TOKEN)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    const after = metrics_memory.snapshot();
    try std.testing.expectEqual(before.hydration_dropped_entries_total, after.hydration_dropped_entries_total);
    try std.testing.expectEqual(before.hydration_dropped_bytes_total, after.hydration_dropped_bytes_total);
}

test "test_hydrate_pins_core_through_the_endpoint" {
    var env = (try setup()) orelse return error.SkipZigTest;
    defer env.deinit();
    // One cold core fact plus five 60_004-byte windowed rows: the 256 KiB
    // window admits the core fact and the four newest windowed rows; the
    // coldest windowed row is the only drop. Pure recency would have dropped
    // the core fact (the oldest row of all) — this pins the tier policy at
    // the real GET endpoint, response body and counters both.
    try seedCoreRow(env, ZID_HYD_CORE, "owner");
    try seedRows(env, ZID_HYD_CORE, HYD_ROWS, HYD_CONTENT_LEN);

    const url = try memoryUrl(ZID_HYD_CORE);
    defer ALLOC.free(url);

    const before = metrics_memory.snapshot();
    const r = try (try env.h.get(url).bearer(RUNNER_TOKEN)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"key\":\"owner\""));
    const after = metrics_memory.snapshot();
    try std.testing.expectEqual(before.hydration_dropped_entries_total + 1, after.hydration_dropped_entries_total);
    try std.testing.expectEqual(before.hydration_dropped_bytes_total + HYD_ROW_BYTES, after.hydration_dropped_bytes_total);
    try std.testing.expectEqual(@as(i64, HYD_ROWS), after.hydration_entries);
}

// ── §capture: cap-eviction counter ──────────────────────────────────────────

test "test_cap_eviction_counter_exact" {
    var env = (try setup()) orelse return error.SkipZigTest;
    defer env.deinit();
    // Fill to exactly the per-fleet cap with cold rows; the two pushed
    // entries land newer, so the backstop evicts the two coldest seeds.
    try seedRows(env, ZID_CAP_OVER, protocol.MAX_MEMORY_ENTRIES_PER_AGENT, 8);

    const body = try buildPush(LEASE_CAP_OVER, "pk", 2, "fresh");
    defer ALLOC.free(body);
    const url = try memoryUrl(ZID_CAP_OVER);
    defer ALLOC.free(url);

    const before = metrics_memory.snapshot();
    const r = try (try (try env.h.post(url).bearer(RUNNER_TOKEN)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"stored\":2"));
    const after = metrics_memory.snapshot();
    try std.testing.expectEqual(before.cap_evictions_total + 2, after.cap_evictions_total);
    // The durable set is back at the cap: evictions removed the overage.
    try std.testing.expectEqual(@as(i64, @intCast(protocol.MAX_MEMORY_ENTRIES_PER_AGENT)), try rowCount(env, ZID_CAP_OVER));
}

test "test_under_cap_no_eviction_count" {
    var env = (try setup()) orelse return error.SkipZigTest;
    defer env.deinit();

    const body = try buildPush(LEASE_CAP_UNDER, "uk", 1, "small");
    defer ALLOC.free(body);
    const url = try memoryUrl(ZID_CAP_UNDER);
    defer ALLOC.free(url);

    const before = metrics_memory.snapshot();
    const r1 = try (try (try env.h.post(url).bearer(RUNNER_TOKEN)).json(body)).send();
    defer r1.deinit();
    try r1.expectStatus(.ok);
    // Idempotency rider: the same push again upserts (no row growth), the
    // capture counter moves consistently with upsert semantics (+1 per push),
    // and eviction stays at zero throughout.
    const r2 = try (try (try env.h.post(url).bearer(RUNNER_TOKEN)).json(body)).send();
    defer r2.deinit();
    try r2.expectStatus(.ok);
    const after = metrics_memory.snapshot();
    try std.testing.expectEqual(before.cap_evictions_total, after.cap_evictions_total);
    try std.testing.expectEqual(before.captured_total + 2, after.captured_total);
    try std.testing.expectEqual(@as(i64, 1), try rowCount(env, ZID_CAP_UNDER));
}

// ── §capture: truncation + per-delta skip counters ──────────────────────────

test "test_capture_truncation_counter" {
    var env = (try setup()) orelse return error.SkipZigTest;
    defer env.deinit();

    const body = try buildPush(LEASE_TRUNC, "tk", TRUNC_DELTAS, TRUNC_CONTENT);
    defer ALLOC.free(body);
    const url = try memoryUrl(ZID_TRUNC);
    defer ALLOC.free(url);

    const stored_fragment = try std.fmt.allocPrint(ALLOC, "\"stored\":{d}", .{TRUNC_KEPT});
    defer ALLOC.free(stored_fragment);

    const before = metrics_memory.snapshot();
    const r = try (try (try env.h.post(url).bearer(RUNNER_TOKEN)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    // The push truncates once at the byte budget; the stored count matches the
    // kept prefix and the persisted rows agree with the response.
    try std.testing.expect(r.bodyContains(stored_fragment));
    const after = metrics_memory.snapshot();
    try std.testing.expectEqual(before.capture_truncated_total + 1, after.capture_truncated_total);
    try std.testing.expectEqual(before.captured_total + TRUNC_KEPT, after.captured_total);
    try std.testing.expectEqual(@as(i64, @intCast(TRUNC_KEPT)), try rowCount(env, ZID_TRUNC));
}

test "test_capture_skip_counter_per_delta" {
    var env = (try setup()) orelse return error.SkipZigTest;
    defer env.deinit();

    // Two invalid deltas (empty key; empty content) + one valid — the skip
    // counter moves per delta and the valid one still persists.
    const body = try std.fmt.allocPrint(
        ALLOC,
        "{{\"lease_id\":\"{s}\",\"fencing_token\":{d},\"memory\":[" ++
            "{{\"key\":\"\",\"content\":\"v\",\"category\":\"c\"}}," ++
            "{{\"key\":\"sk-bad\",\"content\":\"\",\"category\":\"c\"}}," ++
            "{{\"key\":\"sk-ok\",\"content\":\"v\",\"category\":\"c\"}}]}}",
        .{ LEASE_SKIP, FENCE },
    );
    defer ALLOC.free(body);
    const url = try memoryUrl(ZID_SKIP);
    defer ALLOC.free(url);

    const before = metrics_memory.snapshot();
    const r = try (try (try env.h.post(url).bearer(RUNNER_TOKEN)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"stored\":1"));
    try std.testing.expect(r.bodyContains("\"skipped\":2"));
    const after = metrics_memory.snapshot();
    try std.testing.expectEqual(before.capture_skipped_total + 2, after.capture_skipped_total);
    try std.testing.expectEqual(before.captured_total + 1, after.captured_total);
    try std.testing.expectEqual(@as(i64, 1), try rowCount(env, ZID_SKIP));
}

// ── §capture: aged-daily retention sweep ────────────────────────────────────

test "test_capture_sweeps_aged_daily" {
    var env = (try setup()) orelse return error.SkipZigTest;
    defer env.deinit();
    // One daily aged past the retention window, one young daily; the push
    // itself stores a fresh entry. The capture's post-push sweep removes only
    // the aged row — the young daily and the pushed entry survive.
    const now = clock.nowMillis();
    try seedDailyAt(env, ZID_SWEEP, "aged-note", 1, now - memory_adapter.DAILY_RETENTION_MS - 60_000);
    try seedDailyAt(env, ZID_SWEEP, "young-note", 2, now - 60_000);

    const body = try buildPush(LEASE_SWEEP, "wk", 1, "fresh");
    defer ALLOC.free(body);
    const url = try memoryUrl(ZID_SWEEP);
    defer ALLOC.free(url);
    const r = try (try (try env.h.post(url).bearer(RUNNER_TOKEN)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"stored\":1"));
    try std.testing.expectEqual(@as(i64, 2), try rowCount(env, ZID_SWEEP));

    // The survivor set is exactly {young-note, wk01} — visible on hydrate.
    const hr = try (try env.h.get(url).bearer(RUNNER_TOKEN)).send();
    defer hr.deinit();
    try hr.expectStatus(.ok);
    try std.testing.expect(hr.bodyContains("\"key\":\"young-note\""));
    try std.testing.expect(hr.bodyContains("\"key\":\"wk01\""));
    try std.testing.expect(!hr.bodyContains("\"key\":\"aged-note\""));
}

test "test_sweep_frees_cap_slots_before_eviction" {
    var env = (try setup()) orelse return error.SkipZigTest;
    defer env.deinit();
    // At the cap: 999 durable custom rows + 1 aged daily (the NEWEST row of
    // the set). The push puts the fleet one over; the sweep must clear the
    // aged daily BEFORE cap eviction runs, or eviction selects the coldest
    // durable row as victim in the doomed row's place — silent durable loss
    // breaching "only daily expires" via the cap side door.
    try seedRows(env, ZID_SWEEP_CAP, protocol.MAX_MEMORY_ENTRIES_PER_AGENT - 1, 8);
    const now = clock.nowMillis();
    try seedDailyAt(env, ZID_SWEEP_CAP, "aged-note", 1, now - memory_adapter.DAILY_RETENTION_MS - 60_000);

    const body = try buildPush(LEASE_SWEEP_CAP, "fk", 1, "fresh");
    defer ALLOC.free(body);
    const url = try memoryUrl(ZID_SWEEP_CAP);
    defer ALLOC.free(url);

    const before = metrics_memory.snapshot();
    const r = try (try (try env.h.post(url).bearer(RUNNER_TOKEN)).json(body)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(r.bodyContains("\"stored\":1"));
    const after = metrics_memory.snapshot();

    // The sweep freed the slot first: zero cap evictions, the set sits at the
    // cap, the coldest durable row (hk1) survives, the aged daily is gone.
    try std.testing.expectEqual(before.cap_evictions_total, after.cap_evictions_total);
    try std.testing.expectEqual(@as(i64, @intCast(protocol.MAX_MEMORY_ENTRIES_PER_AGENT)), try rowCount(env, ZID_SWEEP_CAP));
    const hr = try (try env.h.get(url).bearer(RUNNER_TOKEN)).send();
    defer hr.deinit();
    try hr.expectStatus(.ok);
    try std.testing.expect(hr.bodyContains("\"key\":\"hk1\""));
    try std.testing.expect(!hr.bodyContains("\"key\":\"aged-note\""));
}

// ── §render: the families are live on the real /metrics scrape ──────────────

test "test_metrics_render_memory_loss_families_http" {
    var env = (try setup()) orelse return error.SkipZigTest;
    defer env.deinit();
    // One hydrate on an over-budget set guarantees memory activity, then the
    // operator-facing scrape must expose every loss family with HELP lines.
    try seedRows(env, ZID_HYD_DROP, HYD_ROWS, HYD_CONTENT_LEN);
    const url = try memoryUrl(ZID_HYD_DROP);
    defer ALLOC.free(url);
    const hr = try (try env.h.get(url).bearer(RUNNER_TOKEN)).send();
    defer hr.deinit();
    try hr.expectStatus(.ok);

    const scrape = try env.h.get("/metrics").send();
    defer scrape.deinit();
    try scrape.expectStatus(.ok);
    try std.testing.expect(scrape.bodyContains("# HELP fleet_memory_hydration_dropped_entries_total "));
    try std.testing.expect(scrape.bodyContains("# HELP fleet_memory_hydration_dropped_bytes_total "));
    try std.testing.expect(scrape.bodyContains("# HELP fleet_memory_cap_evictions_total "));
    try std.testing.expect(scrape.bodyContains("# HELP fleet_memory_capture_truncated_total "));
    try std.testing.expect(scrape.bodyContains("# HELP fleet_memory_capture_skipped_total "));
    try std.testing.expect(scrape.bodyContains("# HELP fleet_memory_search_zero_hits_total "));
}
