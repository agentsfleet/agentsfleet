// Label-placement eligibility over the real HTTP lease path + live test DB +
// Redis. The candidate scan (fleet.assign.listCandidates) admits a zombie for a
// runner only when the zombie's `required_tags` are a subset of the runner's
// advertised `labels`. These tests drive the actual `POST /v1/runners/leases`
// route so the filter, the atomic claim, and the event read are all exercised
// end-to-end — not a re-implemented SELECT.
//
// Coverage:
//   * subset match: a [gpu] zombie leases to a gpu-labelled runner, never to a
//     plain one;
//   * empty required_tags: any runner leases it (back-compat with the global race);
//   * sticky hint never overrides eligibility: an expired affinity pointing at an
//     ineligible runner does not let that runner win; an eligible runner does;
//   * hold then schedule: an unsatisfiable zombie stays unclaimed (the event is
//     NOT consumed during the hold), then leases once a matching runner enrolls.
//
// Mirrors integration_roundtrip_test.zig's harness wiring + seed helpers.
// Requires LIVE_DB=1 + a reachable Redis; skipped when either is missing.

const std = @import("std");
const clock = @import("common").clock;
const pg = @import("pg");
const auth_mw = @import("../auth/middleware/mod.zig");
const serve_runner_lookup = @import("../cmd/serve_runner_lookup.zig");
const api_key = @import("../auth/api_key.zig");
const harness_mod = @import("../http/test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const redis_zombie = @import("../queue/redis_zombie.zig");
const protocol = @import("contract").protocol;
const base = @import("../db/test_fixtures.zig");

const ALLOC = std.testing.allocator;

// UUIDv7 literals (version nibble 7, variant 8) so the schema id CHECKs pass.
// Distinct node suffix (…0e…) from sibling fleet tests to avoid row collision.
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e0011";
const GPU_RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e0a01";
const PLAIN_RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e0b01";
const ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e0c01";
const SESSION_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e0d01";
const AFFINITY_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e0e01";

const GPU_TOKEN = "zrn_" ++ "a" ** 64;
const PLAIN_TOKEN = "zrn_" ++ "b" ** 64;

// Ample balance so per-lease billing never exhausts mid-test.
const LARGE_BALANCE_NANOS: i64 = 1_000_000_000_000;

const CONFIG_NO_GATES =
    \\{"name":"placement-bot","x-usezombie":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"budget":{"daily_dollars":5.0}}}
;
const SOURCE_MD =
    \\---
    \\name: placement-bot
    \\---
    \\
    \\You are a placement test agent.
;

// SAFETY: populated by configureRegistry before the chain reads it.
var runner_lookup_ctx: serve_runner_lookup.Ctx = undefined;

fn configureRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    runner_lookup_ctx = .{ .pool = h.pool };
    reg.runner_bearer_mw = .{ .host = &runner_lookup_ctx, .lookup = serve_runner_lookup.lookup };
}

fn startHarness() !*TestHarness {
    return TestHarness.start(ALLOC, .{ .configureRegistry = configureRegistry });
}

// ── Seed helpers ────────────────────────────────────────────────────────────

/// A runner with explicit JSON `labels` (the capability advertisement matched
/// against a zombie's required_tags). `labels_json` is a literal like '["gpu"]'.
fn seedRunnerWithLabels(conn: *pg.Conn, runner_id: []const u8, host_id: []const u8, token: []const u8, labels_json: []const u8) !void {
    const hash = api_key.sha256Hex(token);
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, status, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, 'dev_none', 'active', $4::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ runner_id, host_id, hash[0..], labels_json });
}

/// Seed the zombie + session, then stamp its required_tags. `tags_literal` is a
/// Postgres TEXT[] literal like '{gpu}' or '{}'. base.seedZombie omits
/// required_tags, so it lands as the column DEFAULT '{}' first; this UPDATE sets
/// the test value.
fn seedZombieWithTags(conn: *pg.Conn, tags_literal: []const u8) !void {
    try base.seedZombie(conn, ZOMBIE_ID, WORKSPACE_ID, "placement-bot", CONFIG_NO_GATES, SOURCE_MD);
    try base.seedZombieSession(conn, SESSION_ID, ZOMBIE_ID, "{}");
    _ = try conn.exec(
        "UPDATE core.zombies SET required_tags = $1::text[] WHERE id = $2::uuid",
        .{ tags_literal, ZOMBIE_ID },
    );
}

/// An expired affinity slot whose sticky hint points at `last_runner_id`.
/// leased_until = 0 ⇒ claimable; used to prove the hint does not override
/// eligibility.
fn seedExpiredAffinity(conn: *pg.Conn, last_runner_id: []const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_affinity
        \\  (id, zombie_id, last_runner_id, fencing_seq, leased_until,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, 1, 0, 0, 0, 0, 0, 0, 0)
        \\ON CONFLICT (zombie_id) DO UPDATE
        \\  SET last_runner_id = EXCLUDED.last_runner_id, fencing_seq = EXCLUDED.fencing_seq,
        \\      leased_until = EXCLUDED.leased_until
    , .{ AFFINITY_ID, ZOMBIE_ID, last_runner_id });
}

fn fundLargeBalance(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO billing.tenant_billing (tenant_id, balance_nanos, grant_source, created_at, updated_at)
        \\VALUES ($1::uuid, $2, 'placement-test', 0, 0)
        \\ON CONFLICT (tenant_id) DO UPDATE
        \\  SET balance_nanos = EXCLUDED.balance_nanos, balance_exhausted_at = NULL
    , .{ base.TEST_TENANT_ID, LARGE_BALANCE_NANOS });
}

fn publishFreshEvent(h: *TestHarness) !void {
    try redis_zombie.ensureZombieConsumerGroup(&h.queue, ZOMBIE_ID);
    const id = try h.queue.xaddZombieEvent(.{
        .event_id = "",
        .zombie_id = ZOMBIE_ID,
        .workspace_id = WORKSPACE_ID,
        .actor = "steer:test-user",
        .event_type = .chat,
        .request_json = "{\"message\":\"ping\"}",
        .created_at = clock.nowMillis(),
    });
    h.queue.alloc.free(id);
}

/// Common seed for every test: tenant, workspace, platform provider, balance,
/// the zombie carrying `tags_json`, and one fresh event. Runners are seeded
/// per-test (their labels are the variable under test).
fn seedBase(h: *TestHarness, conn: *pg.Conn, tags_json: []const u8) !void {
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try base.seedPlatformProvider(ALLOC, conn, WORKSPACE_ID);
    try fundLargeBalance(conn);
    try seedZombieWithTags(conn, tags_json);
    try publishFreshEvent(h);
}

// ── Lease helper ────────────────────────────────────────────────────────────

/// True iff the lease response carried a non-null lease (the runner was
/// assigned the zombie's event). Frees the parsed body internally.
fn leasePresent(h: *TestHarness, token: []const u8) !bool {
    const req = try (try h.post(protocol.PATH_RUNNER_LEASES).bearer(token)).json("{}");
    const resp = try req.send();
    defer resp.deinit();
    try resp.expectStatus(.ok);
    const parsed = try std.json.parseFromSlice(std.json.Value, ALLOC, resp.body, .{});
    defer parsed.deinit();
    const lease = parsed.value.object.get("lease") orelse return false;
    return lease != .null;
}

fn execIgnore(conn: *pg.Conn, sql: []const u8, args: anytype) void {
    _ = conn.exec(sql, args) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

fn delStream(h: *TestHarness, comptime key: []const u8) void {
    var resp = h.queue.command(&.{ "DEL", key }) catch return;
    resp.deinit(h.queue.alloc);
}

fn cleanupAll(h: *TestHarness, conn: *pg.Conn) void {
    delStream(h, "zombie:" ++ ZOMBIE_ID ++ ":events");
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE zombie_id = $1::uuid", .{ZOMBIE_ID});
    execIgnore(conn, "DELETE FROM fleet.runner_affinity WHERE zombie_id = $1::uuid", .{ZOMBIE_ID});
    execIgnore(conn, "DELETE FROM fleet.runners WHERE id IN ($1::uuid, $2::uuid)", .{ GPU_RUNNER_ID, PLAIN_RUNNER_ID });
    execIgnore(conn, "DELETE FROM core.zombie_events WHERE zombie_id = $1::uuid", .{ZOMBIE_ID});
    base.teardownPlatformProvider(conn, WORKSPACE_ID);
    base.teardownZombies(conn, WORKSPACE_ID);
    base.teardownWorkspace(conn, WORKSPACE_ID);
    base.teardownTenant(conn);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

test "claim respects required tag subset: a [gpu] zombie leases only to a gpu runner" {
    const h = startHarness() catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    try seedBase(h, conn, "{gpu}");
    try seedRunnerWithLabels(conn, GPU_RUNNER_ID, "plc-gpu", GPU_TOKEN, "[\"gpu\"]");
    try seedRunnerWithLabels(conn, PLAIN_RUNNER_ID, "plc-plain", PLAIN_TOKEN, "[]");

    // The plain runner does not satisfy [gpu] → never sees the zombie. It must
    // NOT consume the event (so the gpu runner can still claim it next).
    try std.testing.expect(!try leasePresent(h, PLAIN_TOKEN));
    // The gpu runner's labels ⊇ [gpu] → it leases the zombie.
    try std.testing.expect(try leasePresent(h, GPU_TOKEN));
}

test "empty required_tags leases to any runner (back-compat with the global race)" {
    const h = startHarness() catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    try seedBase(h, conn, "{}");
    try seedRunnerWithLabels(conn, PLAIN_RUNNER_ID, "plc-plain", PLAIN_TOKEN, "[]");

    // [] ⊆ any labels ⇒ a label-less runner claims an untagged zombie.
    try std.testing.expect(try leasePresent(h, PLAIN_TOKEN));
}

test "sticky hint never overrides eligibility: an ineligible sticky runner does not win" {
    const h = startHarness() catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    try seedBase(h, conn, "{gpu}");
    try seedRunnerWithLabels(conn, GPU_RUNNER_ID, "plc-gpu", GPU_TOKEN, "[\"gpu\"]");
    try seedRunnerWithLabels(conn, PLAIN_RUNNER_ID, "plc-plain", PLAIN_TOKEN, "[]");
    // The sticky hint points at the PLAIN runner, and the slot is claimable.
    try seedExpiredAffinity(conn, PLAIN_RUNNER_ID);

    // Despite being the sticky last_runner_id, the plain runner is ineligible
    // ([gpu] ⊄ []) → it does not win.
    try std.testing.expect(!try leasePresent(h, PLAIN_TOKEN));
    // The eligible gpu runner wins even though it is not the sticky hint.
    try std.testing.expect(try leasePresent(h, GPU_TOKEN));
}

test "unsatisfiable tags hold then schedule once a matching runner enrolls" {
    const h = startHarness() catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    try seedBase(h, conn, "{gpu}");
    // Only a plain runner exists: the zombie's [gpu] is unsatisfiable → holds.
    try seedRunnerWithLabels(conn, PLAIN_RUNNER_ID, "plc-plain", PLAIN_TOKEN, "[]");
    try std.testing.expect(!try leasePresent(h, PLAIN_TOKEN));

    // A matching runner enrolls → the still-unclaimed zombie schedules. (The
    // event survived the hold because the plain poll never consumed it.)
    try seedRunnerWithLabels(conn, GPU_RUNNER_ID, "plc-gpu", GPU_TOKEN, "[\"gpu\"]");
    try std.testing.expect(try leasePresent(h, GPU_TOKEN));
}
