// Integration tests for the event-lifecycle terminal half: gate
// refusals write `gate_blocked` rows with named failure labels + XACK, the
// guarded transition never reopens a terminal row, the stable consumer
// identity keeps group cardinality flat, and the reclaim sweep recovers
// entries stranded in a dead consumer's Pending Entries List (PEL).
//
// Drives POST /v1/runners/me/leases through the in-process TestHarness
// against the live test DB + Redis (skipped when either is missing), and
// calls `event_rows.markBlocked` directly for row-level invariants.
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
const redis_fleet = @import("../queue/redis_fleet.zig");
const approval_gate_async = @import("../fleet_runtime/approval_gate_async.zig");
const event_rows = @import("event_rows.zig");

const ALLOC = std.testing.allocator;

pub const WORKSPACE_ID = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7011";
pub const RUNNER_ID = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7a01";
const AGENTSFLEET_CRED = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7c01";
const AGENTSFLEET_PROVIDER = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7c02";
const AGENTSFLEET_GATED = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7c03";
pub const FLEET_IDLE = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7c04";
pub const AGENTSFLEET_STRAND = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7c05";
pub const AGENTSFLEET_ROW = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7c06";
pub const AGENTSFLEET_REACK = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7c07";
pub const AGENTSFLEET_GATED_EXP = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7c08";
pub const AGENTSFLEET_RECLAIM_FAIL = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7c09";
pub const AGENTSFLEET_FRESH_FAIL = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7c0a";
pub const AGENTSFLEET_RELEASE_FAIL = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7c0b";
const SESSION_BASE = "0195c9da-1e2a-7f13-8abc-2b3e1e0d7d0";

const RUNNER_TOKEN = auth_mw.runner_bearer.RUNNER_TOKEN_PREFIX ++ "e" ** 64;
const DEAD_CONSUMER = "worker-retired-host-1700000000000";
/// Idle injected onto a stranded entry — must exceed the reclaim min-idle.
pub const FORCED_IDLE_MS = queue_consts.fleet_xautoclaim_min_idle_ms_int * 2;

pub const CONFIG_PLAIN =
    \\{"name":"lifecycle-plain","x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"budget":{"daily_dollars":5.0}}}
;
const CONFIG_GHOST_CRED =
    \\{"name":"lifecycle-cred","x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"credentials":["ghost_cred"],"budget":{"daily_dollars":5.0}}}
;
const CONFIG_GATED_ALL =
    \\{"name":"lifecycle-gated","x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"budget":{"daily_dollars":5.0},"gates":{"rules":[{"tool":"*","action":"*","behavior":"approve"}],"timeout_ms":1800000}}}
;
// A 1ms approval deadline so the gate expires deterministically between two
// polls (the second poll is a full HTTP round-trip past the deadline).
pub const CONFIG_GATED_FAST =
    \\{"name":"lifecycle-gatex","x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"budget":{"daily_dollars":5.0},"gates":{"rules":[{"tool":"*","action":"*","behavior":"approve"}],"timeout_ms":1}}}
;
const SOURCE_MD =
    \\---
    \\name: lifecycle-bot
    \\---
    \\
    \\You are an event-lifecycle test fleet.
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

pub fn seedFleetWithConfig(conn: *pg.Conn, fleet_id: []const u8, name: []const u8, config: []const u8, session_suffix: []const u8) !void {
    try base.seedFleet(conn, fleet_id, WORKSPACE_ID, name, config, SOURCE_MD);
    var sid_buf: [64]u8 = undefined;
    const sid = try std.fmt.bufPrint(&sid_buf, "{s}{s}", .{ SESSION_BASE, session_suffix });
    try base.seedFleetSession(conn, sid, fleet_id, "{}");
}

pub const Env = struct {
    h: *TestHarness,

    pub fn deinit(self: *Env) void {
        if (self.h.acquireConn()) |conn| {
            defer self.h.releaseConn(conn);
            cleanupRows(conn);
            base.teardownFleets(conn, WORKSPACE_ID);
            base.teardownPlatformProvider(conn, WORKSPACE_ID);
            base.teardownWorkspace(conn, WORKSPACE_ID);
            _ = conn.exec("DELETE FROM fleet.runners WHERE id = $1::uuid", .{RUNNER_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
        } else |_| {}
        deleteStream(self.h, AGENTSFLEET_CRED);
        deleteStream(self.h, AGENTSFLEET_PROVIDER);
        deleteStream(self.h, AGENTSFLEET_GATED);
        deleteStream(self.h, FLEET_IDLE);
        deleteStream(self.h, AGENTSFLEET_STRAND);
        deleteStream(self.h, AGENTSFLEET_ROW);
        deleteStream(self.h, AGENTSFLEET_REACK);
        deleteStream(self.h, AGENTSFLEET_GATED_EXP);
        deleteStream(self.h, AGENTSFLEET_RECLAIM_FAIL);
        deleteStream(self.h, AGENTSFLEET_FRESH_FAIL);
        deleteStream(self.h, AGENTSFLEET_RELEASE_FAIL);
        self.h.deinit();
    }
};

fn cleanupRows(conn: *pg.Conn) void {
    // The approval-denial test leaves a gate row, and fleet_approval_gates
    // is append-only — DELETE raises via trigger, so a surviving row
    // FK-blocks teardownFleets → teardownWorkspace → every later
    // teardownTenant of the shared TEST_TENANT (billing rows then leak
    // across suites). TRUNCATE bypasses row-level triggers; no test depends
    // on pre-existing gate rows (each seeds its own).
    _ = conn.exec("TRUNCATE core.fleet_approval_gates", .{}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM fleet.runner_leases WHERE workspace_id = $1::uuid", .{WORKSPACE_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM fleet.runner_affinity WHERE fleet_id IN (SELECT id FROM core.fleets WHERE workspace_id = $1::uuid)", .{WORKSPACE_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.fleet_events WHERE workspace_id = $1::uuid", .{WORKSPACE_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
}

fn deleteStream(h: *TestHarness, fleet_id: []const u8) void {
    var key_buf: [128]u8 = undefined;
    const key = std.fmt.bufPrint(&key_buf, "fleet:{s}:events", .{fleet_id}) catch return;
    var resp = h.queue.commandAllowError(&.{ "DEL", key }) catch return;
    resp.deinit(h.queue.alloc);
}

/// Start the harness + seed the canonical fixture set. Skips when DB or
/// Redis is unavailable. `TestHarness.start` already cleared any leaked
/// fault-injection constraint (`test_fixtures.dropInjectedFaultConstraints`)
/// before this runs.
pub fn setup() !Env {
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

pub fn publishEvent(h: *TestHarness, fleet_id: []const u8) ![]const u8 {
    try redis_fleet.ensureFleetConsumerGroup(&h.queue, fleet_id);
    return h.queue.xaddFleetEvent(.{
        .event_id = "",
        .fleet_id = fleet_id,
        .workspace_id = WORKSPACE_ID,
        .actor = "steer:test-user",
        .event_type = .chat,
        .request_json = "{\"message\":\"ping\"}",
        .created_at = clock.nowMillis(),
    });
}

/// One lease poll; returns true when a lease was issued.
pub fn pollLease(h: *TestHarness) !bool {
    const req = try (try h.post(protocol.PATH_RUNNER_LEASES).bearer(RUNNER_TOKEN)).json("{}");
    const resp = try req.send();
    defer resp.deinit();
    try resp.expectStatus(.ok);
    return std.mem.indexOf(u8, resp.body, "\"lease\":null") == null;
}

const RowView = struct { status_buf: [32]u8, status_len: usize, label_buf: [64]u8, label_len: usize };

/// Null when no row exists for (fleet_id, event_id).
fn eventRow(conn: *pg.Conn, fleet_id: []const u8, event_id: []const u8) !?RowView {
    var q = PgQuery.from(try conn.query(
        \\SELECT status, COALESCE(failure_label, '') FROM core.fleet_events
        \\WHERE fleet_id = $1::uuid AND event_id = $2
    , .{ fleet_id, event_id }));
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

pub fn expectRow(conn: *pg.Conn, fleet_id: []const u8, event_id: []const u8, status: []const u8, label: []const u8) !void {
    const row = (try eventRow(conn, fleet_id, event_id)) orelse return error.EventRowMissing;
    try std.testing.expectEqualStrings(status, row.status_buf[0..row.status_len]);
    try std.testing.expectEqualStrings(label, row.label_buf[0..row.label_len]);
}

pub fn pendingCount(h: *TestHarness, fleet_id: []const u8) !i64 {
    var key_buf: [128]u8 = undefined;
    const key = try std.fmt.bufPrint(&key_buf, "fleet:{s}:events", .{fleet_id});
    var resp = try h.queue.command(&.{ "XPENDING", key, queue_consts.fleet_consumer_group });
    defer resp.deinit(h.queue.alloc);
    const arr = resp.array orelse return error.RedisUnexpectedResponse;
    return switch (arr[0]) {
        .integer => |n| n,
        else => error.RedisUnexpectedResponse,
    };
}

pub fn consumerCount(h: *TestHarness, fleet_id: []const u8) !usize {
    var key_buf: [128]u8 = undefined;
    const key = try std.fmt.bufPrint(&key_buf, "fleet:{s}:events", .{fleet_id});
    var resp = try h.queue.command(&.{ "XINFO", "CONSUMERS", key, queue_consts.fleet_consumer_group });
    defer resp.deinit(h.queue.alloc);
    const arr = resp.array orelse return error.RedisUnexpectedResponse;
    return arr.len;
}

/// Deliver the stream's next entry to a throwaway consumer name (the retired
/// per-probe minting), simulating a stranded delivery.
pub fn deliverToDeadConsumer(h: *TestHarness, fleet_id: []const u8) !void {
    var key_buf: [128]u8 = undefined;
    const key = try std.fmt.bufPrint(&key_buf, "fleet:{s}:events", .{fleet_id});
    var resp = try h.queue.command(&.{
        "XREADGROUP", "GROUP", queue_consts.fleet_consumer_group, DEAD_CONSUMER,
        "COUNT",      "1",     "STREAMS",                         key,
        ">",
    });
    resp.deinit(h.queue.alloc);
}

/// Force an entry's idle clock via XCLAIM IDLE so the reclaim bound is
/// crossed without waiting wall-clock minutes.
pub fn forceIdle(h: *TestHarness, fleet_id: []const u8, event_id: []const u8, idle_ms: i64) !void {
    var key_buf: [128]u8 = undefined;
    const key = try std.fmt.bufPrint(&key_buf, "fleet:{s}:events", .{fleet_id});
    var idle_buf: [24]u8 = undefined;
    const idle = try std.fmt.bufPrint(&idle_buf, "{d}", .{idle_ms});
    var resp = try h.queue.command(&.{
        "XCLAIM", key,      queue_consts.fleet_consumer_group, DEAD_CONSUMER,
        "0",      event_id, "IDLE",                            idle,
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
    try seedFleetWithConfig(conn, AGENTSFLEET_CRED, "lifecycle-cred", CONFIG_GHOST_CRED, "1");

    const event_id = try publishEvent(h, AGENTSFLEET_CRED);
    defer h.queue.alloc.free(event_id);

    // The fleet declares a credential that is not in the vault: no lease
    // ships with a null secrets map (RULE ESO) — terminal row + XACK instead.
    try std.testing.expect(!try pollLease(h));
    try expectRow(conn, AGENTSFLEET_CRED, event_id, event_rows.STATUS_GATE_BLOCKED, event_rows.LABEL_SECRET_MISSING);
    try std.testing.expectEqual(@as(i64, 0), try pendingCount(h, AGENTSFLEET_CRED));
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
    try seedFleetWithConfig(conn, AGENTSFLEET_PROVIDER, "lifecycle-prov", CONFIG_PLAIN, "2");
    // self-managed row whose secret_ref has no vault row →
    // error.SecretMissing → permanent refusal (RULE ECL).
    _ = try conn.exec(
        \\INSERT INTO core.tenant_model_selection
        \\  (tenant_id, mode, provider, model, context_cap_tokens, secret_ref, created_at, updated_at)
        \\VALUES ($1::uuid, 'self_managed', 'fireworks', 'test-model', 256000, 'no-such-cred', $2, $2)
        \\ON CONFLICT (tenant_id) DO UPDATE SET mode = EXCLUDED.mode, secret_ref = EXCLUDED.secret_ref
    , .{ base.TEST_TENANT_ID, clock.nowMillis() });

    const event_id = try publishEvent(h, AGENTSFLEET_PROVIDER);
    defer h.queue.alloc.free(event_id);

    try std.testing.expect(!try pollLease(h));
    try expectRow(conn, AGENTSFLEET_PROVIDER, event_id, event_rows.STATUS_GATE_BLOCKED, event_rows.LABEL_TENANT_RESOLVE_FAILED);
    try std.testing.expectEqual(@as(i64, 0), try pendingCount(h, AGENTSFLEET_PROVIDER));
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
    try seedFleetWithConfig(conn, AGENTSFLEET_GATED, "lifecycle-gated", CONFIG_GATED_ALL, "3");

    const event_id = try publishEvent(h, AGENTSFLEET_GATED);
    defer h.queue.alloc.free(event_id);

    // Poll 1: the gate parks the event pending — no lease, no terminal row,
    // entry retained in the PEL for re-evaluation.
    try std.testing.expect(!try pollLease(h));
    try expectRow(conn, AGENTSFLEET_GATED, event_id, event_rows.STATUS_RECEIVED, "");
    try std.testing.expectEqual(@as(i64, 1), try pendingCount(h, AGENTSFLEET_GATED));

    // A human denies: write the decision the approval webhook would write.
    const maybe_ref = try approval_gate_async.lookupEventGateRef(&h.queue, AGENTSFLEET_GATED, event_id);
    const ref = maybe_ref orelse return error.GateRefMissing;
    var key_buf: [256]u8 = undefined;
    const decision_key = try std.fmt.bufPrint(&key_buf, "{s}{s}", .{ ec.GATE_RESPONSE_KEY_PREFIX, ref.actionId() });
    try h.queue.setEx(decision_key, ec.GATE_DECISION_DENY, 60);

    // Poll 2: the PEL re-delivers, the recorded gate resolves denied →
    // terminal row + XACK (the async-gate outcome, persisted as a row).
    try std.testing.expect(!try pollLease(h));
    try expectRow(conn, AGENTSFLEET_GATED, event_id, event_rows.STATUS_GATE_BLOCKED, event_rows.LABEL_APPROVAL_DENIED);
    try std.testing.expectEqual(@as(i64, 0), try pendingCount(h, AGENTSFLEET_GATED));
}
