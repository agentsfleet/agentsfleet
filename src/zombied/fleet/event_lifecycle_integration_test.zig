// Integration tests for the event-lifecycle terminal half: gate
// refusals write `gate_blocked` rows with named failure labels + XACK, the
// guarded transition never reopens a terminal row, the stable consumer
// identity keeps group cardinality flat, and the reclaim sweep recovers
// entries stranded in a dead consumer's Pending Entries List (PEL).
//
// Drives POST /v1/runners/me/leases through the in-process TestHarness
// against the live test DB + Redis (skipped when either is missing), and
// calls `event_rows.markBlocked` / `reclaim_sweeper.sweepOnce` directly for
// the row- and sweep-level invariants.
//
// The balance-exhausted HTTP path (spec 1.1) is unreachable while the free
// trial window keeps every charge at zero (billing_and_provider_keys.md §
// free-trial gate: "the HTTP-path gate integration tests skip while the
// window is open"); the row mechanics + label spelling are pinned here via
// markBlocked, and the gate wiring is exercised through the same blockEvent
// path by the other refusals.

const std = @import("std");
const clock = @import("common").clock;
const pg = @import("pg");
const auth_mw = @import("../auth/middleware/mod.zig");
const serve_runner_lookup = @import("../cmd/serve_runner_lookup.zig");
const api_key = @import("../auth/api_key.zig");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const harness_mod = @import("../http/test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const protocol = @import("contract").protocol;
const base = @import("../db/test_fixtures.zig");
const ec = @import("../errors/error_registry.zig");
const queue_consts = @import("../queue/constants.zig");
const redis_zombie = @import("../queue/redis_zombie.zig");
const approval_gate_async = @import("../zombie/approval_gate_async.zig");
const event_rows = @import("event_rows.zig");
const reclaim_sweeper = @import("reclaim_sweeper.zig");

const ALLOC = std.testing.allocator;

const WORKSPACE_ID = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7011";
const RUNNER_ID = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7a01";
const ZOMBIE_CRED = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7c01";
const ZOMBIE_PROVIDER = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7c02";
const ZOMBIE_GATED = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7c03";
const ZOMBIE_IDLE = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7c04";
const ZOMBIE_STRAND = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7c05";
const ZOMBIE_ROW = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7c06";
const SESSION_BASE = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7d0";

const RUNNER_TOKEN = "zrn_" ++ "e" ** 64;
const DEAD_CONSUMER = "worker-retired-host-1700000000000";
/// Idle injected onto a stranded entry — must exceed the reclaim min-idle.
const FORCED_IDLE_MS = queue_consts.zombie_xautoclaim_min_idle_ms_int * 2;

const CONFIG_PLAIN =
    \\{"name":"lifecycle-plain","x-usezombie":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"budget":{"daily_dollars":5.0}}}
;
const CONFIG_GHOST_CRED =
    \\{"name":"lifecycle-cred","x-usezombie":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"credentials":["ghost_cred"],"budget":{"daily_dollars":5.0}}}
;
const CONFIG_GATED_ALL =
    \\{"name":"lifecycle-gated","x-usezombie":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"budget":{"daily_dollars":5.0},"gates":{"rules":[{"tool":"*","action":"*","behavior":"approve"}],"timeout_ms":1800000}}}
;
const SOURCE_MD =
    \\---
    \\name: lifecycle-bot
    \\---
    \\
    \\You are an event-lifecycle test agent.
;

// SAFETY: populated by configureRegistry before the middleware chain reads it.
var runner_lookup_ctx: serve_runner_lookup.Ctx = undefined;

fn configureRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    runner_lookup_ctx = .{ .pool = h.pool };
    reg.runner_bearer_mw = .{ .host = &runner_lookup_ctx, .lookup = serve_runner_lookup.lookup };
}

// ── Seed + teardown ─────────────────────────────────────────────────────────

fn seedRunner(conn: *pg.Conn) !void {
    const hash = api_key.sha256Hex(RUNNER_TOKEN);
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, 'lifecycle-host', $2, 'dev_none', 'active', '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ RUNNER_ID, hash[0..] });
}

fn seedZombieWithConfig(conn: *pg.Conn, zombie_id: []const u8, name: []const u8, config: []const u8, session_suffix: []const u8) !void {
    try base.seedZombie(conn, zombie_id, WORKSPACE_ID, name, config, SOURCE_MD);
    var sid_buf: [64]u8 = undefined;
    const sid = try std.fmt.bufPrint(&sid_buf, "{s}{s}", .{ SESSION_BASE, session_suffix });
    try base.seedZombieSession(conn, sid, zombie_id, "{}");
}

const Env = struct {
    h: *TestHarness,

    fn deinit(self: *Env) void {
        if (self.h.acquireConn()) |conn| {
            defer self.h.releaseConn(conn);
            cleanupRows(conn);
            base.teardownZombies(conn, WORKSPACE_ID);
            base.teardownPlatformProvider(conn, WORKSPACE_ID);
            base.teardownWorkspace(conn, WORKSPACE_ID);
            _ = conn.exec("DELETE FROM fleet.runners WHERE id = $1::uuid", .{RUNNER_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
        } else |_| {}
        deleteStream(self.h, ZOMBIE_CRED);
        deleteStream(self.h, ZOMBIE_PROVIDER);
        deleteStream(self.h, ZOMBIE_GATED);
        deleteStream(self.h, ZOMBIE_IDLE);
        deleteStream(self.h, ZOMBIE_STRAND);
        self.h.deinit();
    }
};

fn cleanupRows(conn: *pg.Conn) void {
    // The approval-denial test leaves a gate row, and zombie_approval_gates
    // is append-only — DELETE raises via trigger, so a surviving row
    // FK-blocks teardownZombies → teardownWorkspace → every later
    // teardownTenant of the shared TEST_TENANT (billing rows then leak
    // across suites). TRUNCATE bypasses row-level triggers; no test depends
    // on pre-existing gate rows (each seeds its own).
    _ = conn.exec("TRUNCATE core.zombie_approval_gates", .{}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM fleet.runner_leases WHERE workspace_id = $1::uuid", .{WORKSPACE_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM fleet.runner_affinity WHERE zombie_id IN (SELECT id FROM core.zombies WHERE workspace_id = $1::uuid)", .{WORKSPACE_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.zombie_events WHERE workspace_id = $1::uuid", .{WORKSPACE_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
}

fn deleteStream(h: *TestHarness, zombie_id: []const u8) void {
    var key_buf: [128]u8 = undefined;
    const key = std.fmt.bufPrint(&key_buf, "zombie:{s}:events", .{zombie_id}) catch return;
    var resp = h.queue.commandAllowError(&.{ "DEL", key }) catch return;
    resp.deinit(h.queue.alloc);
}

/// Start the harness + seed the canonical fixture set. Skips when DB or
/// Redis is unavailable.
fn setup() !Env {
    const h = try TestHarness.start(ALLOC, .{ .configureRegistry = configureRegistry });
    errdefer h.deinit();
    if (!h.tryConnectRedis()) return error.SkipZigTest;
    base.setTestEncryptionKey();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try base.seedPlatformProvider(ALLOC, conn, WORKSPACE_ID);
    try seedRunner(conn);
    return .{ .h = h };
}

// ── Redis + HTTP helpers ────────────────────────────────────────────────────

fn publishEvent(h: *TestHarness, zombie_id: []const u8) ![]const u8 {
    try redis_zombie.ensureZombieConsumerGroup(&h.queue, zombie_id);
    return h.queue.xaddZombieEvent(.{
        .event_id = "",
        .zombie_id = zombie_id,
        .workspace_id = WORKSPACE_ID,
        .actor = "steer:test-user",
        .event_type = .chat,
        .request_json = "{\"message\":\"ping\"}",
        .created_at = clock.nowMillis(),
    });
}

/// One lease poll; returns true when a lease was issued.
fn pollLease(h: *TestHarness) !bool {
    const req = try (try h.post(protocol.PATH_RUNNER_LEASES).bearer(RUNNER_TOKEN)).json("{}");
    const resp = try req.send();
    defer resp.deinit();
    try resp.expectStatus(.ok);
    return std.mem.indexOf(u8, resp.body, "\"lease\":null") == null;
}

const RowView = struct { status_buf: [32]u8, status_len: usize, label_buf: [64]u8, label_len: usize };

/// Null when no row exists for (zombie_id, event_id).
fn eventRow(conn: *pg.Conn, zombie_id: []const u8, event_id: []const u8) !?RowView {
    var q = PgQuery.from(try conn.query(
        \\SELECT status, COALESCE(failure_label, '') FROM core.zombie_events
        \\WHERE zombie_id = $1::uuid AND event_id = $2
    , .{ zombie_id, event_id }));
    defer q.deinit();
    const row = (try q.next()) orelse return null;
    // SAFETY: both buffers are fully written below before any read.
    var out = RowView{ .status_buf = undefined, .status_len = 0, .label_buf = undefined, .label_len = 0 };
    const status = try row.get([]const u8, 0);
    const label = try row.get([]const u8, 1);
    @memcpy(out.status_buf[0..status.len], status);
    out.status_len = status.len;
    @memcpy(out.label_buf[0..label.len], label);
    out.label_len = label.len;
    return out;
}

fn expectRow(conn: *pg.Conn, zombie_id: []const u8, event_id: []const u8, status: []const u8, label: []const u8) !void {
    const row = (try eventRow(conn, zombie_id, event_id)) orelse return error.EventRowMissing;
    try std.testing.expectEqualStrings(status, row.status_buf[0..row.status_len]);
    try std.testing.expectEqualStrings(label, row.label_buf[0..row.label_len]);
}

fn pendingCount(h: *TestHarness, zombie_id: []const u8) !i64 {
    var key_buf: [128]u8 = undefined;
    const key = try std.fmt.bufPrint(&key_buf, "zombie:{s}:events", .{zombie_id});
    var resp = try h.queue.command(&.{ "XPENDING", key, queue_consts.zombie_consumer_group });
    defer resp.deinit(h.queue.alloc);
    const arr = resp.array orelse return error.RedisUnexpectedResponse;
    return switch (arr[0]) {
        .integer => |n| n,
        else => error.RedisUnexpectedResponse,
    };
}

fn consumerCount(h: *TestHarness, zombie_id: []const u8) !usize {
    var key_buf: [128]u8 = undefined;
    const key = try std.fmt.bufPrint(&key_buf, "zombie:{s}:events", .{zombie_id});
    var resp = try h.queue.command(&.{ "XINFO", "CONSUMERS", key, queue_consts.zombie_consumer_group });
    defer resp.deinit(h.queue.alloc);
    const arr = resp.array orelse return error.RedisUnexpectedResponse;
    return arr.len;
}

/// Deliver the stream's next entry to a throwaway consumer name (the retired
/// per-probe minting), simulating a stranded delivery.
fn deliverToDeadConsumer(h: *TestHarness, zombie_id: []const u8) !void {
    var key_buf: [128]u8 = undefined;
    const key = try std.fmt.bufPrint(&key_buf, "zombie:{s}:events", .{zombie_id});
    var resp = try h.queue.command(&.{
        "XREADGROUP", "GROUP", queue_consts.zombie_consumer_group, DEAD_CONSUMER,
        "COUNT",      "1",     "STREAMS",                          key,
        ">",
    });
    resp.deinit(h.queue.alloc);
}

/// Force an entry's idle clock via XCLAIM IDLE so the reclaim bound is
/// crossed without waiting wall-clock minutes.
fn forceIdle(h: *TestHarness, zombie_id: []const u8, event_id: []const u8, idle_ms: i64) !void {
    var key_buf: [128]u8 = undefined;
    const key = try std.fmt.bufPrint(&key_buf, "zombie:{s}:events", .{zombie_id});
    var idle_buf: [24]u8 = undefined;
    const idle = try std.fmt.bufPrint(&idle_buf, "{d}", .{idle_ms});
    var resp = try h.queue.command(&.{
        "XCLAIM", key,      queue_consts.zombie_consumer_group, DEAD_CONSUMER,
        "0",      event_id, "IDLE",                             idle,
    });
    resp.deinit(h.queue.alloc);
}

// ── §1 — terminal writes ────────────────────────────────────────────────────

test "missing declared secret refuses the lease: gate_blocked + secret_missing + XACK" {
    var env = setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedZombieWithConfig(conn, ZOMBIE_CRED, "lifecycle-cred", CONFIG_GHOST_CRED, "1");

    const event_id = try publishEvent(h, ZOMBIE_CRED);
    defer h.queue.alloc.free(event_id);

    // The zombie declares a credential that is not in the vault: no lease
    // ships with a null secrets map (RULE ESO) — terminal row + XACK instead.
    try std.testing.expect(!try pollLease(h));
    try expectRow(conn, ZOMBIE_CRED, event_id, event_rows.STATUS_GATE_BLOCKED, event_rows.LABEL_SECRET_MISSING);
    try std.testing.expectEqual(@as(i64, 0), try pendingCount(h, ZOMBIE_CRED));
}

test "unresolvable provider credential blocks the event: gate_blocked + tenant_resolve_failed + XACK" {
    var env = setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedZombieWithConfig(conn, ZOMBIE_PROVIDER, "lifecycle-prov", CONFIG_PLAIN, "2");
    // self-managed row whose credential_ref has no vault row →
    // error.CredentialMissing → permanent refusal (RULE ECL).
    _ = try conn.exec(
        \\INSERT INTO core.tenant_providers
        \\  (tenant_id, mode, provider, model, context_cap_tokens, credential_ref, created_at, updated_at)
        \\VALUES ($1::uuid, 'self_managed', 'fireworks', 'test-model', 256000, 'no-such-cred', $2, $2)
        \\ON CONFLICT (tenant_id) DO UPDATE SET mode = EXCLUDED.mode, credential_ref = EXCLUDED.credential_ref
    , .{ base.TEST_TENANT_ID, clock.nowMillis() });

    const event_id = try publishEvent(h, ZOMBIE_PROVIDER);
    defer h.queue.alloc.free(event_id);

    try std.testing.expect(!try pollLease(h));
    try expectRow(conn, ZOMBIE_PROVIDER, event_id, event_rows.STATUS_GATE_BLOCKED, event_rows.LABEL_TENANT_RESOLVE_FAILED);
    try std.testing.expectEqual(@as(i64, 0), try pendingCount(h, ZOMBIE_PROVIDER));
}

test "approval denial writes the terminal row: gate_blocked + approval_denied + XACK" {
    var env = setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedZombieWithConfig(conn, ZOMBIE_GATED, "lifecycle-gated", CONFIG_GATED_ALL, "3");

    const event_id = try publishEvent(h, ZOMBIE_GATED);
    defer h.queue.alloc.free(event_id);

    // Poll 1: the gate parks the event pending — no lease, no terminal row,
    // entry retained in the PEL for re-evaluation.
    try std.testing.expect(!try pollLease(h));
    try expectRow(conn, ZOMBIE_GATED, event_id, event_rows.STATUS_RECEIVED, "");
    try std.testing.expectEqual(@as(i64, 1), try pendingCount(h, ZOMBIE_GATED));

    // A human denies: write the decision the approval webhook would write.
    const maybe_ref = try approval_gate_async.lookupEventGateRef(&h.queue, ZOMBIE_GATED, event_id);
    const ref = maybe_ref orelse return error.GateRefMissing;
    var key_buf: [256]u8 = undefined;
    const decision_key = try std.fmt.bufPrint(&key_buf, "{s}{s}", .{ ec.GATE_RESPONSE_KEY_PREFIX, ref.actionId() });
    try h.queue.setEx(decision_key, ec.GATE_DECISION_DENY, 60);

    // Poll 2: the PEL re-delivers, the recorded gate resolves denied →
    // terminal row + XACK (the async-gate outcome, persisted as a row).
    try std.testing.expect(!try pollLease(h));
    try expectRow(conn, ZOMBIE_GATED, event_id, event_rows.STATUS_GATE_BLOCKED, event_rows.LABEL_APPROVAL_DENIED);
    try std.testing.expectEqual(@as(i64, 0), try pendingCount(h, ZOMBIE_GATED));
}

test "markBlocked is guarded: terminal rows never reopen, second transition affects zero rows" {
    var env = setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    // zombie_events.zombie_id references core.zombies — the row's zombie must exist.
    try seedZombieWithConfig(conn, ZOMBIE_ROW, "lifecycle-row", CONFIG_PLAIN, "6");
    const EVENT_ID = "1700000000000-7";
    _ = try conn.exec(
        \\INSERT INTO core.zombie_events
        \\  (uid, zombie_id, event_id, workspace_id, actor, event_type, status,
        \\   request_json, created_at, updated_at)
        \\VALUES ('0195c9da-1e2a-7f13-8abc-2b3e1e0d7e01'::uuid, $1::uuid, $2, $3::uuid,
        \\        'steer:test', 'chat', $4, '{}'::jsonb, 0, 0)
        \\ON CONFLICT (zombie_id, event_id) DO UPDATE SET status = EXCLUDED.status, failure_label = NULL
    , .{ ZOMBIE_ROW, EVENT_ID, WORKSPACE_ID, event_rows.STATUS_RECEIVED });

    // First transition: received → gate_blocked (balance label spelling is
    // pinned by billing_and_provider_keys.md).
    try std.testing.expectEqual(@as(i64, 1), try event_rows.markBlocked(h.pool, ZOMBIE_ROW, EVENT_ID, event_rows.LABEL_BALANCE_EXHAUSTED));
    try expectRow(conn, ZOMBIE_ROW, EVENT_ID, event_rows.STATUS_GATE_BLOCKED, event_rows.LABEL_BALANCE_EXHAUSTED);

    // Second transition attempt (any label): zero rows — terminal is final.
    try std.testing.expectEqual(@as(i64, 0), try event_rows.markBlocked(h.pool, ZOMBIE_ROW, EVENT_ID, event_rows.LABEL_APPROVAL_DENIED));
    try expectRow(conn, ZOMBIE_ROW, EVENT_ID, event_rows.STATUS_GATE_BLOCKED, event_rows.LABEL_BALANCE_EXHAUSTED);
}

// ── §2 — stable identity + reclaim ──────────────────────────────────────────

test "consumer identity is stable: repeated idle probes leave one consumer in the group" {
    var env = setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedZombieWithConfig(conn, ZOMBIE_IDLE, "lifecycle-idle", CONFIG_PLAIN, "4");
    try redis_zombie.ensureZombieConsumerGroup(&h.queue, ZOMBIE_IDLE);

    var i: usize = 0;
    while (i < 25) : (i += 1) _ = try pollLease(h);
    try std.testing.expectEqual(@as(usize, 1), try consumerCount(h, ZOMBIE_IDLE));
}

test "reclaim sweep recovers a stranded delivery from a dead consumer and re-leases it" {
    var env = setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedZombieWithConfig(conn, ZOMBIE_STRAND, "lifecycle-strand", CONFIG_PLAIN, "5");

    const event_id = try publishEvent(h, ZOMBIE_STRAND);
    defer h.queue.alloc.free(event_id);
    try deliverToDeadConsumer(h, ZOMBIE_STRAND);
    try forceIdle(h, ZOMBIE_STRAND, event_id, FORCED_IDLE_MS);

    const stats = try reclaim_sweeper.sweepOnce(h.pool, &h.queue, ALLOC);
    try std.testing.expect(stats.reclaimed_entries >= 1);

    // The recovered entry re-enters the lease flow on the next poll.
    try std.testing.expect(try pollLease(h));
    try expectRow(conn, ZOMBIE_STRAND, event_id, event_rows.STATUS_RECEIVED, "");
}

test "reclaim sweep never touches an entry inside the lease window" {
    var env = setup() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer env.deinit();
    const h = env.h;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedZombieWithConfig(conn, ZOMBIE_STRAND, "lifecycle-strand", CONFIG_PLAIN, "5");

    const event_id = try publishEvent(h, ZOMBIE_STRAND);
    defer h.queue.alloc.free(event_id);
    try deliverToDeadConsumer(h, ZOMBIE_STRAND); // idle ≈ 0 — under the bound

    const stats = try reclaim_sweeper.sweepOnce(h.pool, &h.queue, ALLOC);
    try std.testing.expectEqual(@as(i64, 0), stats.reclaimed_entries);
    try std.testing.expectEqual(@as(i64, 1), try pendingCount(h, ZOMBIE_STRAND));
}

test "reclaim min-idle exceeds the lease window" {
    // pin test: the comptime assertion in queue/constants.zig enforces this;
    // the runtime pin makes the relation visible in the test inventory.
    try std.testing.expect(queue_consts.zombie_xautoclaim_min_idle_ms_int > @import("common").LEASE_TTL_MS);
}
