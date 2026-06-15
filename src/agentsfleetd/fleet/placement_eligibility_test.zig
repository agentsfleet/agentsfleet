// Label-placement eligibility over the real HTTP lease path + live test DB +
// Redis. The candidate scan (fleet.assign.listCandidates) admits a agent for a
// runner only when the agent's `required_tags` are a subset of the runner's
// advertised `labels`. These tests drive the actual `POST /v1/runners/leases`
// route so the filter, the atomic claim, and the event read are all exercised
// end-to-end — not a re-implemented SELECT.
//
// Coverage:
//   * subset match: a [gpu] agent leases to a gpu-labelled runner, never to a
//     plain one;
//   * empty required_tags: any runner leases it (back-compat with the global race);
//   * sticky hint never overrides eligibility: an expired affinity pointing at an
//     ineligible runner does not let that runner win; an eligible runner does;
//   * hold then schedule: an unsatisfiable agent stays unclaimed (the event is
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
const redis_agent = @import("../queue/redis_agent.zig");
const protocol = @import("contract").protocol;
const base = @import("../db/test_fixtures.zig");
const id_format = @import("../types/id_format.zig");

const ALLOC = std.testing.allocator;

// UUIDv7 literals (version nibble 7, variant 8) so the schema id CHECKs pass.
// Distinct node suffix (…0e…) from sibling fleet tests to avoid row collision.
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e0011";
const GPU_RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e0a01";
const PLAIN_RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e0b01";
const AGENTSFLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e0c01";
const SESSION_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e0d01";
const AFFINITY_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e0e01";
// Second agent + an arm runner for the complex two-agent routing test.
const AGENT2_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e0f01";
const SESSION2_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e1001";
const ARM_RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0e1101";

const GPU_TOKEN = "zrn_" ++ "a" ** 64;
const PLAIN_TOKEN = "zrn_" ++ "b" ** 64;
const ARM_TOKEN = "zrn_" ++ "c" ** 64;

// Concurrent racers are generated at runtime with this host prefix so cleanup
// can delete them by LIKE without tracking each id.
const CONC_HOST_PREFIX = "plc-conc-";

// Ample balance so per-lease billing never exhausts mid-test.
const LARGE_BALANCE_NANOS: i64 = 1_000_000_000_000;

const CONFIG_NO_GATES =
    \\{"name":"placement-bot","x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"budget":{"daily_dollars":5.0}}}
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
/// against a agent's required_tags). `labels_json` is a literal like '["gpu"]'.
fn seedRunnerWithLabels(conn: *pg.Conn, runner_id: []const u8, host_id: []const u8, token: []const u8, labels_json: []const u8) !void {
    const hash = api_key.sha256Hex(token);
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, 'dev_none', 'active', $4::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ runner_id, host_id, hash[0..], labels_json });
}

/// Seed the agent + session, then stamp its required_tags. `tags_literal` is a
/// Postgres TEXT[] literal like '{gpu}' or '{}'. base.seedAgent omits
/// required_tags, so it lands as the column DEFAULT '{}' first; this UPDATE sets
/// the test value.
fn seedAgentWithTags(conn: *pg.Conn, tags_literal: []const u8) !void {
    try base.seedAgent(conn, AGENTSFLEET_ID, WORKSPACE_ID, "placement-bot", CONFIG_NO_GATES, SOURCE_MD);
    try base.seedAgentSession(conn, SESSION_ID, AGENTSFLEET_ID, "{}");
    _ = try conn.exec(
        "UPDATE core.agents SET required_tags = $1::text[] WHERE id = $2::uuid",
        .{ tags_literal, AGENTSFLEET_ID },
    );
}

/// An expired affinity slot whose sticky hint points at `last_runner_id`.
/// leased_until = 0 ⇒ claimable; used to prove the hint does not override
/// eligibility.
fn seedExpiredAffinity(conn: *pg.Conn, last_runner_id: []const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_affinity
        \\  (id, agent_id, last_runner_id, fencing_seq, leased_until,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, 1, 0, 0, 0, 0, 0, 0, 0)
        \\ON CONFLICT (agent_id) DO UPDATE
        \\  SET last_runner_id = EXCLUDED.last_runner_id, fencing_seq = EXCLUDED.fencing_seq,
        \\      leased_until = EXCLUDED.leased_until
    , .{ AFFINITY_ID, AGENTSFLEET_ID, last_runner_id });
}

fn fundLargeBalance(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO billing.tenant_billing (tenant_id, balance_nanos, grant_source, created_at, updated_at)
        \\VALUES ($1::uuid, $2, 'placement-test', 0, 0)
        \\ON CONFLICT (tenant_id) DO UPDATE
        \\  SET balance_nanos = EXCLUDED.balance_nanos, balance_exhausted_at = NULL
    , .{ base.TEST_TENANT_ID, LARGE_BALANCE_NANOS });
}

fn publishEventFor(h: *TestHarness, agent_id: []const u8) !void {
    try redis_agent.ensureAgentConsumerGroup(&h.queue, agent_id);
    const id = try h.queue.xaddAgentEvent(.{
        .event_id = "",
        .agent_id = agent_id,
        .workspace_id = WORKSPACE_ID,
        .actor = "steer:test-user",
        .event_type = .chat,
        .request_json = "{\"message\":\"ping\"}",
        .created_at = clock.nowMillis(),
    });
    h.queue.alloc.free(id);
}

/// Seed a second tagged agent (AGENT2_ID) + its session + a fresh event, for
/// the two-agent routing test. `tags_literal` is a TEXT[] literal.
fn seedSecondAgent(h: *TestHarness, conn: *pg.Conn, name: []const u8, tags_literal: []const u8) !void {
    try base.seedAgent(conn, AGENT2_ID, WORKSPACE_ID, name, CONFIG_NO_GATES, SOURCE_MD);
    try base.seedAgentSession(conn, SESSION2_ID, AGENT2_ID, "{}");
    _ = try conn.exec(
        "UPDATE core.agents SET required_tags = $1::text[] WHERE id = $2::uuid",
        .{ tags_literal, AGENT2_ID },
    );
    try publishEventFor(h, AGENT2_ID);
}

/// Seed a runtime-generated racer with the `CONC_HOST_PREFIX` host so cleanup
/// can sweep by prefix. Returns the owned bearer token (caller frees).
fn seedConcurrentRunner(conn: *pg.Conn, idx: usize, gpu: bool) ![]const u8 {
    const rid = try id_format.generateRunnerId(ALLOC);
    defer ALLOC.free(rid);
    const host = try std.fmt.allocPrint(ALLOC, CONC_HOST_PREFIX ++ "{d}", .{idx});
    defer ALLOC.free(host);
    const token = try std.fmt.allocPrint(ALLOC, "zrn_{s}{d:0>60}", .{ if (gpu) "gpu" else "pln", idx });
    errdefer ALLOC.free(token);
    try seedRunnerWithLabels(conn, rid, host, token, if (gpu) "[\"gpu\"]" else "[]");
    return token;
}

/// One racer's outcome: 0 = errored, 1 = got a lease, 2 = no lease.
const LeaseSlot = struct { code: u8 = 0 };

const Racer = struct {
    fn run(h: *TestHarness, token: []const u8, slot: *LeaseSlot) void {
        const present = leasePresent(h, token) catch {
            slot.* = .{ .code = 0 };
            return;
        };
        slot.* = .{ .code = if (present) 1 else 2 };
    }
};

/// Common seed for every test: tenant, workspace, platform provider, balance,
/// the agent carrying `tags_json`, and one fresh event. Runners are seeded
/// per-test (their labels are the variable under test).
fn seedBase(h: *TestHarness, conn: *pg.Conn, tags_json: []const u8) !void {
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try base.seedPlatformProvider(ALLOC, conn, WORKSPACE_ID);
    try fundLargeBalance(conn);
    try seedAgentWithTags(conn, tags_json);
    try publishEventFor(h, AGENTSFLEET_ID);
}

// ── Lease helper ────────────────────────────────────────────────────────────

/// True iff the lease response carried a non-null lease (the runner was
/// assigned the agent's event). Frees the parsed body internally.
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
    delStream(h, "agent:" ++ AGENTSFLEET_ID ++ ":events");
    delStream(h, "agent:" ++ AGENT2_ID ++ ":events");
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE agent_id IN ($1::uuid, $2::uuid)", .{ AGENTSFLEET_ID, AGENT2_ID });
    execIgnore(conn, "DELETE FROM fleet.runner_affinity WHERE agent_id IN ($1::uuid, $2::uuid)", .{ AGENTSFLEET_ID, AGENT2_ID });
    execIgnore(conn, "DELETE FROM fleet.runners WHERE id IN ($1::uuid, $2::uuid, $3::uuid)", .{ GPU_RUNNER_ID, PLAIN_RUNNER_ID, ARM_RUNNER_ID });
    execIgnore(conn, "DELETE FROM fleet.runners WHERE host_id LIKE 'plc-conc-%'", .{});
    execIgnore(conn, "DELETE FROM core.agent_events WHERE agent_id IN ($1::uuid, $2::uuid)", .{ AGENTSFLEET_ID, AGENT2_ID });
    base.teardownPlatformProvider(conn, WORKSPACE_ID);
    base.teardownAgents(conn, WORKSPACE_ID);
    base.teardownWorkspace(conn, WORKSPACE_ID);
    base.teardownTenant(conn);
}

// ── Tests ─────────────────────────────────────────────────────────────────────

test "claim respects required tag subset: a [gpu] agent leases only to a gpu runner" {
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

    // The plain runner does not satisfy [gpu] → never sees the agent. It must
    // NOT consume the event (so the gpu runner can still claim it next).
    try std.testing.expect(!try leasePresent(h, PLAIN_TOKEN));
    // The gpu runner's labels ⊇ [gpu] → it leases the agent.
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

    // [] ⊆ any labels ⇒ a label-less runner claims an untagged agent.
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
    // Only a plain runner exists: the agent's [gpu] is unsatisfiable → holds.
    try seedRunnerWithLabels(conn, PLAIN_RUNNER_ID, "plc-plain", PLAIN_TOKEN, "[]");
    try std.testing.expect(!try leasePresent(h, PLAIN_TOKEN));

    // A matching runner enrolls → the still-unclaimed agent schedules. (The
    // event survived the hold because the plain poll never consumed it.)
    try seedRunnerWithLabels(conn, GPU_RUNNER_ID, "plc-gpu", GPU_TOKEN, "[\"gpu\"]");
    try std.testing.expect(try leasePresent(h, GPU_TOKEN));
}

// ── Edge cases ────────────────────────────────────────────────────────────────

test "edge: a runner missing one of two required tags is ineligible" {
    const h = startHarness() catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    try seedBase(h, conn, "{gpu,us-east}");
    // Partial: has gpu but not us-east → {gpu,us-east} ⊄ {gpu}.
    try seedRunnerWithLabels(conn, PLAIN_RUNNER_ID, "plc-partial", PLAIN_TOKEN, "[\"gpu\"]");
    // Full: has both.
    try seedRunnerWithLabels(conn, GPU_RUNNER_ID, "plc-full", GPU_TOKEN, "[\"gpu\",\"us-east\"]");

    try std.testing.expect(!try leasePresent(h, PLAIN_TOKEN));
    try std.testing.expect(try leasePresent(h, GPU_TOKEN));
}

test "edge: a runner whose labels are a superset of the required tags is eligible" {
    const h = startHarness() catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    try seedBase(h, conn, "{gpu}");
    // More labels than required — still a superset of {gpu}.
    try seedRunnerWithLabels(conn, GPU_RUNNER_ID, "plc-super", GPU_TOKEN, "[\"gpu\",\"us-east\",\"arm64\"]");

    try std.testing.expect(try leasePresent(h, GPU_TOKEN));
}

test "edge: tag matching is exact-string (case-sensitive) — GPU does not satisfy gpu" {
    const h = startHarness() catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    try seedBase(h, conn, "{GPU}"); // upper-case required tag
    // Lower-case label does NOT match the upper-case requirement.
    try seedRunnerWithLabels(conn, PLAIN_RUNNER_ID, "plc-lower", PLAIN_TOKEN, "[\"gpu\"]");
    // Exact-case label does.
    try seedRunnerWithLabels(conn, GPU_RUNNER_ID, "plc-exact", GPU_TOKEN, "[\"GPU\"]");

    try std.testing.expect(!try leasePresent(h, PLAIN_TOKEN));
    try std.testing.expect(try leasePresent(h, GPU_TOKEN));
}

// ── Concurrency ─────────────────────────────────────────────────────────────────

test "concurrent: eligible runners race one tagged agent — exactly one wins, ineligible never do" {
    const h = startHarness() catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    // One [gpu] agent carrying exactly ONE event.
    try seedBase(h, conn, "{gpu}");

    const N_GPU = 3;
    const N = 6; // first N_GPU are gpu (eligible), the rest plain (ineligible)
    var tokens: [N][]const u8 = undefined;
    var seeded: usize = 0;
    defer for (tokens[0..seeded]) |t| ALLOC.free(t);
    for (0..N) |i| {
        tokens[i] = try seedConcurrentRunner(conn, i, i < N_GPU);
        seeded += 1;
    }

    // All six poll the lease endpoint simultaneously.
    var slots: [N]LeaseSlot = @splat(LeaseSlot{});
    var threads: [N]std.Thread = undefined;
    for (&threads, 0..) |*t, i| {
        t.* = try std.Thread.spawn(.{}, Racer.run, .{ h, tokens[i], &slots[i] });
    }
    for (threads) |t| t.join();

    var present: usize = 0;
    var winner: usize = N;
    for (slots, 0..) |s, i| switch (s.code) {
        0 => return error.RacerErrored,
        1 => {
            present += 1;
            winner = i;
        },
        else => {},
    };
    // One event ⇒ exactly one lease, even under the 6-way race; the slot claim
    // admits a single winner and the ineligible plain runners never compete.
    try std.testing.expectEqual(@as(usize, 1), present);
    // And the winner is one of the eligible (gpu) runners — never a plain one.
    try std.testing.expect(winner < N_GPU);
}

// ── Complex routing ─────────────────────────────────────────────────────────────

test "complex: two tagged agents route only to their matching runner" {
    const h = startHarness() catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupAll(h, conn);

    // Z_GPU [gpu] (the default AGENTSFLEET_ID) + Z_ARM [arm64] (AGENT2_ID), each
    // with its own event; one gpu runner and one arm runner.
    try seedBase(h, conn, "{gpu}");
    try seedSecondAgent(h, conn, "placement-arm-bot", "{arm64}");
    try seedRunnerWithLabels(conn, GPU_RUNNER_ID, "plc-gpu", GPU_TOKEN, "[\"gpu\"]");
    try seedRunnerWithLabels(conn, ARM_RUNNER_ID, "plc-arm", ARM_TOKEN, "[\"arm64\"]");

    // The gpu runner gets its agent...
    try std.testing.expect(try leasePresent(h, GPU_TOKEN));
    // ...and a second poll yields nothing: Z_ARM is ineligible to it and Z_GPU's
    // event is consumed. So the gpu runner is bounded to its own tag.
    try std.testing.expect(!try leasePresent(h, GPU_TOKEN));
    // The arm runner still gets Z_ARM — proving the gpu runner never reached
    // across to consume the other agent's event.
    try std.testing.expect(try leasePresent(h, ARM_TOKEN));
}
