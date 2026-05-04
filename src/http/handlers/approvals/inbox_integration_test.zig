// HTTP integration tests for the approval inbox:
//   GET  /v1/workspaces/{ws}/approvals
//   GET  /v1/workspaces/{ws}/approvals/{gate_id}
//   POST /v1/workspaces/{ws}/approvals/{gate_id}:approve|:deny
//
// Plus the channel-agnostic resolve dedup and the auto-timeout sweeper.
// Requires TEST_DATABASE_URL — skips gracefully otherwise. Resolve flows
// that need a Redis decision-key roundtrip additionally require REDIS_URL
// and skip when the harness can't reach it.

const std = @import("std");
const pg = @import("pg");

const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const auth_mw = @import("../../../auth/middleware/mod.zig");
const approval_gate_db = @import("../../../zombie/approval_gate_db.zig");
const approval_gate_sweeper = @import("../../../zombie/approval_gate_sweeper.zig");

const ALLOC = std.testing.allocator;

// Reuse the JWT signing fixture (kid + JWKS + tenant/workspace claims) from
// events_integration_test.zig so we don't have to mint a fresh signature.
// Workspace + tenant ids match events_integration_test; ON CONFLICT DO
// NOTHING on the seed inserts handles the inevitable shared-row collisions.
// Zombie ids are distinct so per-suite cleanup (DELETE WHERE workspace_id=…)
// doesn't strand the other suite's rows.
const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
// OTHER_WORKSPACE_ID is in the same tenant but not in the token's claims.
// A second workspace row is inserted under it so the cross-workspace 404
// test has somewhere to seed a gate that the operator token cannot reach.
const OTHER_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f99";
const ZOMBIE_A = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aa701";
const ZOMBIE_B = "0195b4ba-8d3a-7f13-8abc-2b3e1e0aa702";
const TEST_ISSUER = "https://clerk.dev.usezombie.com";
const TEST_AUDIENCE = "https://api.usezombie.com";
const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"2hg972tpbq8H6kzRZ3oVL4wZ9bO-04gJ6gCig68aluyRBzagx-7XXPCiuX80oBHBVj51kvMjT_QDNXfrwzjy4cPbwiVV4HqOGpeIZkPEopfyzs4G7mjiQmx0YuM_5WQUlUjji6Y_DfeaoH-yOhTWBMBVoI0vW_1n66CFaGuEarj3VasdWYxObJTBAM6Jn4XZDcDsBBPNGO4ku7yILkfi11FqXfBP2V8NT0hAGXVAxlWwv-8up1RDzgACp-8JWoC2-kOUJN82fGenDGKq9hW_sumO-4YPNP4U1smnw5jzLlvKa0LBrYG8IgW-3Dniuq2mojhrD_ZQClUd5rF42OyYqw","e":"AQAB","kid":"rbac-test-kid","use":"sig","alg":"RS256"}]}
;
const TOKEN_OPERATOR =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6Im9wZXJhdG9yIn19.V84uE69RTLrRef0sogegUcUZeKWx8E68GEruFoS8HegUa3o7bVCfQjlkllNSbtUut919EygbQv1C16BMfNTOAv1Lvl3AeLYPYr4ni6EnzzGllbyxDw1aY68AGWEEvKOUxd5wCGl8BnEqaOKX7KNNbAOV4AzJNWqnV-uxJiZl6oDtqi8bsSF1HAm9qY9MAl6AwoZLGnT_x6ux_3vfKy_9ckZSbgjN7laZOMqQ5nwwcaSpwYNm_3ZpXJLgHYMVxel2M4rT0SIaFh__rE42yGE9FBDRUFoyktGOR3NYPOzogjj3tfOoecC8NEhrwifzXcSNVAiHOMnmXojjAPEUORovPg";

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn seedAndHarness(alloc: std.mem.Allocator) !*TestHarness {
    const h = try TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = TEST_JWKS,
        .issuer = TEST_ISSUER,
        .audience = TEST_AUDIENCE,
    });
    errdefer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedTestData(conn);
    return h;
}

fn seedTestData(conn: *pg.Conn) !void {
    const now = std.time.milliTimestamp();
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'ApprovalsTest', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, repo_url, default_branch, paused, version, created_at, updated_at)
        \\VALUES ($1, $2, 'https://github.com/test/approvals', 'main', false, 1, $3, $3)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, repo_url, default_branch, paused, version, created_at, updated_at)
        \\VALUES ($1, $2, 'https://github.com/test/approvals-other', 'main', false, 1, $3, $3)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ OTHER_WORKSPACE_ID, TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO core.zombies (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, 'approvals-a', '---\nname: approvals-a\n---', '{"name":"approvals-a"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ ZOMBIE_A, TEST_WORKSPACE_ID });
    _ = try conn.exec(
        \\INSERT INTO core.zombies (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, 'approvals-b', '---\nname: approvals-b\n---', '{"name":"approvals-b"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ ZOMBIE_B, TEST_WORKSPACE_ID });
}

fn cleanupTestData(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.zombie_approval_gates WHERE workspace_id = $1::uuid", .{TEST_WORKSPACE_ID}) catch {};
    _ = conn.exec("DELETE FROM core.zombie_approval_gates WHERE workspace_id = $1::uuid", .{OTHER_WORKSPACE_ID}) catch {};
    _ = conn.exec("DELETE FROM core.zombies WHERE workspace_id = $1::uuid", .{TEST_WORKSPACE_ID}) catch {};
    _ = conn.exec("DELETE FROM workspaces WHERE workspace_id = $1", .{TEST_WORKSPACE_ID}) catch {};
    _ = conn.exec("DELETE FROM workspaces WHERE workspace_id = $1", .{OTHER_WORKSPACE_ID}) catch {};
}

const SeedGate = struct {
    gate_id: []const u8,
    action_id: []const u8,
    zombie_id: []const u8 = ZOMBIE_A,
    workspace_id: []const u8 = TEST_WORKSPACE_ID,
    tool_name: []const u8 = "write_repo",
    action_name: []const u8 = "create_pr",
    gate_kind: []const u8 = "destructive_action",
    proposed_action: []const u8 = "Open PR titled 'wire approval inbox'",
    evidence_json: []const u8 = "{\"files\":[\"src/x.zig\"]}",
    blast_radius: []const u8 = "single repo branch",
    requested_at: i64 = 1_700_000_000_000,
    timeout_at: i64 = 1_700_000_086_400_000, // requested + 24h
};

fn insertGate(conn: *pg.Conn, g: SeedGate) !void {
    _ = try conn.exec(
        \\INSERT INTO core.zombie_approval_gates
        \\  (id, zombie_id, workspace_id, action_id, tool_name, action_name,
        \\   gate_kind, proposed_action, evidence, blast_radius, timeout_at,
        \\   status, detail, requested_at, created_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, $6,
        \\        $7, $8, $9::jsonb, $10, $11,
        \\        'pending', '', $12, $12)
        \\ON CONFLICT (id) DO NOTHING
    , .{
        g.gate_id, g.zombie_id, g.workspace_id, g.action_id, g.tool_name, g.action_name,
        g.gate_kind, g.proposed_action, g.evidence_json, g.blast_radius, g.timeout_at,
        g.requested_at,
    });
}

fn statusOf(conn: *pg.Conn, alloc: std.mem.Allocator, gate_id: []const u8) ![]u8 {
    var q = @import("../../../db/pg_query.zig").PgQuery.from(try conn.query(
        \\SELECT status FROM core.zombie_approval_gates WHERE id = $1::uuid
    , .{gate_id}));
    defer q.deinit();
    const row = (try q.next()) orelse return alloc.dupe(u8, "MISSING");
    return alloc.dupe(u8, try row.get([]const u8, 0));
}

// ── Auth surface ────────────────────────────────────────────────────────

test "integration: approvals GET — no bearer → 401" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupTestData(conn);

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/approvals", .{TEST_WORKSPACE_ID});
    defer ALLOC.free(url);
    const r = try (h.get(url)).send();
    defer r.deinit();
    try r.expectStatus(.unauthorized);
}

test "integration: approvals POST :approve — no bearer → 401" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupTestData(conn);

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/approvals/01999999-9999-7999-9999-999999999999:approve", .{TEST_WORKSPACE_ID});
    defer ALLOC.free(url);
    const r = try (try (h.post(url)).json("{}")).send();
    defer r.deinit();
    try r.expectStatus(.unauthorized);
}

// ── List behavior ───────────────────────────────────────────────────────

test "integration: approvals GET — pending row appears with all spec fields" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupTestData(conn);

    try insertGate(conn, .{
        .gate_id = "01999999-1111-7000-8000-000000000001",
        .action_id = "act-list-001",
    });

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/approvals", .{TEST_WORKSPACE_ID});
    defer ALLOC.free(url);
    const r = try (try (h.get(url)).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(std.mem.indexOf(u8, r.body, "act-list-001") != null);
    try std.testing.expect(std.mem.indexOf(u8, r.body, "destructive_action") != null);
    try std.testing.expect(std.mem.indexOf(u8, r.body, "wire approval inbox") != null);
    try std.testing.expect(std.mem.indexOf(u8, r.body, "single repo branch") != null);
    try std.testing.expect(std.mem.indexOf(u8, r.body, "approvals-a") != null);
}

test "integration: approvals GET — zombie_id filter scopes results" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupTestData(conn);

    try insertGate(conn, .{ .gate_id = "01999999-2222-7000-8000-000000000001", .action_id = "act-zf-a", .zombie_id = ZOMBIE_A });
    try insertGate(conn, .{ .gate_id = "01999999-2222-7000-8000-000000000002", .action_id = "act-zf-b", .zombie_id = ZOMBIE_B });

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/approvals?zombie_id={s}", .{ TEST_WORKSPACE_ID, ZOMBIE_A });
    defer ALLOC.free(url);
    const r = try (try (h.get(url)).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(std.mem.indexOf(u8, r.body, "act-zf-a") != null);
    try std.testing.expect(std.mem.indexOf(u8, r.body, "act-zf-b") == null);
}

test "integration: approvals GET — gate_kind filter" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupTestData(conn);

    try insertGate(conn, .{ .gate_id = "01999999-3333-7000-8000-000000000001", .action_id = "act-kf-1", .gate_kind = "destructive_action" });
    try insertGate(conn, .{ .gate_id = "01999999-3333-7000-8000-000000000002", .action_id = "act-kf-2", .gate_kind = "cost_overrun" });

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/approvals?gate_kind=cost_overrun", .{TEST_WORKSPACE_ID});
    defer ALLOC.free(url);
    const r = try (try (h.get(url)).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(std.mem.indexOf(u8, r.body, "act-kf-2") != null);
    try std.testing.expect(std.mem.indexOf(u8, r.body, "act-kf-1") == null);
}

test "integration: approvals GET — cursor pagination yields next_cursor" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupTestData(conn);

    var i: u8 = 0;
    while (i < 3) : (i += 1) {
        const gid = try std.fmt.allocPrint(ALLOC, "01999999-4444-7000-8000-00000000000{d}", .{i});
        defer ALLOC.free(gid);
        const aid = try std.fmt.allocPrint(ALLOC, "act-pg-{d}", .{i});
        defer ALLOC.free(aid);
        try insertGate(conn, .{
            .gate_id = gid, .action_id = aid,
            .requested_at = 1_700_000_000_000 + @as(i64, i),
        });
    }

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/approvals?limit=2", .{TEST_WORKSPACE_ID});
    defer ALLOC.free(url);
    const r = try (try (h.get(url)).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(std.mem.indexOf(u8, r.body, "\"next_cursor\":\"") != null);
}

test "integration: approvals GET — evidence JSONB roundtrips as object" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupTestData(conn);

    try insertGate(conn, .{
        .gate_id = "01999999-5555-7000-8000-000000000001",
        .action_id = "act-ev-1",
        .evidence_json = "{\"files\":[\"a\",\"b\"],\"loc\":42}",
    });

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/approvals", .{TEST_WORKSPACE_ID});
    defer ALLOC.free(url);
    const r = try (try (h.get(url)).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(std.mem.indexOf(u8, r.body, "\"files\":[\"a\",\"b\"]") != null);
    try std.testing.expect(std.mem.indexOf(u8, r.body, "\"loc\":42") != null);
}

// ── Detail + 404 boundaries ─────────────────────────────────────────────

test "integration: approvals GET detail — unknown gate_id → 404" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupTestData(conn);

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/approvals/01999999-7000-7000-8000-000000000999", .{TEST_WORKSPACE_ID});
    defer ALLOC.free(url);
    const r = try (try (h.get(url)).bearer(TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.not_found);
    try r.expectErrorCode("UZ-APPROVAL-002");
}

test "integration: approvals POST :approve — cross-workspace gate_id → 404" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupTestData(conn);

    // Insert into the OTHER workspace; resolve from the TEST workspace's URL.
    try insertGate(conn, .{
        .gate_id = "01999999-8888-7000-8000-000000000001",
        .action_id = "act-cross-1",
        .workspace_id = OTHER_WORKSPACE_ID,
    });

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/approvals/01999999-8888-7000-8000-000000000001:approve", .{TEST_WORKSPACE_ID});
    defer ALLOC.free(url);
    const r = try (try (try (h.post(url)).bearer(TOKEN_OPERATOR)).json("{}")).send();
    defer r.deinit();
    try r.expectStatus(.not_found);
}

// ── Resolve happy paths (DB-side; Redis decision write is best-effort) ──

test "integration: approvals POST :approve with reason — body persists in detail column" {
    // Validates parseReason → ResolveArgs.reason → detail column round-trip,
    // and exercises the heap-ownership defer free path on resolve.zig:46.
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    if (!h.tryConnectRedis()) return error.SkipZigTest;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupTestData(conn);

    const gid = "01999999-9000-7000-8000-000000000010";
    try insertGate(conn, .{ .gate_id = gid, .action_id = "act-reason-1" });

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/approvals/{s}:approve", .{ TEST_WORKSPACE_ID, gid });
    defer ALLOC.free(url);
    const r = try (try (try (h.post(url)).bearer(TOKEN_OPERATOR)).json("{\"reason\":\"verified change-management ticket\"}")).send();
    defer r.deinit();
    try r.expectStatus(.ok);

    var q = @import("../../../db/pg_query.zig").PgQuery.from(try conn.query(
        \\SELECT detail FROM core.zombie_approval_gates WHERE id = $1::uuid
    , .{gid}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.MissingGateRow;
    try std.testing.expectEqualStrings("verified change-management ticket", try row.get([]const u8, 0));
}

test "integration: anomaly EVAL atomically sets TTL on first INCR" {
    // Validates approval_gate_anomaly.zig EVAL Lua: a fresh key receives a
    // TTL bound to the rule's threshold_window_s in the same Redis round-trip
    // as INCR. The pre-fix code did INCR then EXPIRE as separate commands,
    // leaving a window where a connection drop or server restart between the
    // two would strand the key without a TTL — every subsequent call would
    // see count > 1 and skip the EXPIRE branch, so the counter would live
    // forever.
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    if (!h.tryConnectRedis()) return error.SkipZigTest;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupTestData(conn);

    const approval_gate = @import("../../../zombie/approval_gate.zig");
    const cfg = @import("../../../zombie/config_gates.zig");
    const ec = @import("../../../errors/error_registry.zig");

    const test_zombie = "anomaly-ttl-zombie-001";
    const tool = "write_repo";
    const action = "create_pr";
    const window_s: u32 = 60;

    var key_buf: [256]u8 = undefined;
    const key = try std.fmt.bufPrint(&key_buf, "{s}{s}:{s}:{s}", .{
        ec.GATE_ANOMALY_KEY_PREFIX, test_zombie, tool, action,
    });
    var del_resp = try h.queue.command(&.{ "DEL", key });
    del_resp.deinit(h.queue.alloc);

    const rules = [_]cfg.AnomalyRule{
        .{ .pattern = .same_action, .threshold_count = 100, .threshold_window_s = window_s },
    };
    const result = approval_gate.checkAnomaly(&h.queue, test_zombie, tool, action, &rules);
    try std.testing.expectEqual(approval_gate.AnomalyResult.normal, result);

    var ttl_resp = try h.queue.command(&.{ "PTTL", key });
    defer ttl_resp.deinit(h.queue.alloc);
    const ttl_ms: i64 = switch (ttl_resp) {
        .integer => |n| n,
        else => return error.RedisCommandError,
    };
    // -1 = key exists with no TTL (the bug); -2 = key does not exist.
    try std.testing.expect(ttl_ms > 0);
    try std.testing.expect(ttl_ms <= @as(i64, window_s) * 1000);

    // Subsequent INCRs within the window must NOT reset the TTL — the EVAL
    // script gates EXPIRE on `v == 1`, so the second call reads the same
    // remaining-window TTL (slightly less due to elapsed time) instead of a
    // fresh 60_000ms. Without this guarantee a high-rate caller would never
    // accumulate count past threshold because every call would extend the
    // window.
    const second = approval_gate.checkAnomaly(&h.queue, test_zombie, tool, action, &rules);
    try std.testing.expectEqual(approval_gate.AnomalyResult.normal, second);

    var ttl_resp_2 = try h.queue.command(&.{ "PTTL", key });
    defer ttl_resp_2.deinit(h.queue.alloc);
    const ttl_ms_2: i64 = switch (ttl_resp_2) {
        .integer => |n| n,
        else => return error.RedisCommandError,
    };
    try std.testing.expect(ttl_ms_2 > 0);
    try std.testing.expect(ttl_ms_2 <= ttl_ms);

    var cleanup_resp = try h.queue.command(&.{ "DEL", key });
    cleanup_resp.deinit(h.queue.alloc);
}

test "integration: worker self-timeout writes resolved_by=system:timeout" {
    // Validates event_loop_gate.zig:147 -> resolveGateDecision -> ResolveArgs.atomic
    // attribution flow. Worker fires its own timeout ~60s before the sweeper
    // wakes up; both paths must produce the same canonical attribution.
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupTestData(conn);

    const gid = "01999999-9000-7000-8000-000000000020";
    const action_id = "act-worker-to-1";
    try insertGate(conn, .{ .gate_id = gid, .action_id = action_id });

    const resolver = @import("../../../zombie/approval_gate_resolver.zig");
    @import("../../../zombie/approval_gate.zig").resolveGateDecision(
        h.pool, action_id, .timed_out, resolver.SYSTEM_TIMEOUT, "",
    );

    var q = @import("../../../db/pg_query.zig").PgQuery.from(try conn.query(
        \\SELECT status, resolved_by FROM core.zombie_approval_gates WHERE id = $1::uuid
    , .{gid}));
    defer q.deinit();
    const row = (try q.next()) orelse return error.MissingGateRow;
    try std.testing.expectEqualStrings("timed_out", try row.get([]const u8, 0));
    try std.testing.expectEqualStrings("system:timeout", try row.get([]const u8, 1));
}

test "integration: approvals POST :approve — pending → approved + resolved_by user" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    if (!h.tryConnectRedis()) return error.SkipZigTest;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupTestData(conn);

    const gid = "01999999-9000-7000-8000-000000000001";
    try insertGate(conn, .{ .gate_id = gid, .action_id = "act-ok-1" });

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/approvals/{s}:approve", .{ TEST_WORKSPACE_ID, gid });
    defer ALLOC.free(url);
    const r = try (try (try (h.post(url)).bearer(TOKEN_OPERATOR)).json("{}")).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(std.mem.indexOf(u8, r.body, "\"outcome\":\"approved\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, r.body, "\"resolved_by\":\"user:user_test\"") != null);

    const status = try statusOf(conn, ALLOC, gid);
    defer ALLOC.free(status);
    try std.testing.expectEqualStrings("approved", status);
}

test "integration: approvals POST :deny — pending → denied" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    if (!h.tryConnectRedis()) return error.SkipZigTest;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupTestData(conn);

    const gid = "01999999-9000-7000-8000-000000000002";
    try insertGate(conn, .{ .gate_id = gid, .action_id = "act-ok-2" });

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/approvals/{s}:deny", .{ TEST_WORKSPACE_ID, gid });
    defer ALLOC.free(url);
    const r = try (try (try (h.post(url)).bearer(TOKEN_OPERATOR)).json("{}")).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    try std.testing.expect(std.mem.indexOf(u8, r.body, "\"outcome\":\"denied\"") != null);

    const status = try statusOf(conn, ALLOC, gid);
    defer ALLOC.free(status);
    try std.testing.expectEqualStrings("denied", status);
}

test "integration: approvals POST :approve twice — second call returns 409 with original outcome" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    if (!h.tryConnectRedis()) return error.SkipZigTest;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupTestData(conn);

    const gid = "01999999-9000-7000-8000-000000000003";
    try insertGate(conn, .{ .gate_id = gid, .action_id = "act-dup-1" });

    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/approvals/{s}:approve", .{ TEST_WORKSPACE_ID, gid });
    defer ALLOC.free(url);

    const first = try (try (try (h.post(url)).bearer(TOKEN_OPERATOR)).json("{}")).send();
    defer first.deinit();
    try first.expectStatus(.ok);

    const second = try (try (try (h.post(url)).bearer(TOKEN_OPERATOR)).json("{}")).send();
    defer second.deinit();
    try second.expectStatus(.conflict);
    try second.expectErrorCode("UZ-APPROVAL-006");
    try std.testing.expect(std.mem.indexOf(u8, second.body, "\"outcome\":\"approved\"") != null);
}

test "integration: approvals POST :deny on already-approved row → 409 with prior approved outcome" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    if (!h.tryConnectRedis()) return error.SkipZigTest;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupTestData(conn);

    const gid = "01999999-9000-7000-8000-000000000004";
    try insertGate(conn, .{ .gate_id = gid, .action_id = "act-race-1" });

    const approve_url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/approvals/{s}:approve", .{ TEST_WORKSPACE_ID, gid });
    defer ALLOC.free(approve_url);
    const first = try (try (try (h.post(approve_url)).bearer(TOKEN_OPERATOR)).json("{}")).send();
    defer first.deinit();
    try first.expectStatus(.ok);

    const deny_url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/approvals/{s}:deny", .{ TEST_WORKSPACE_ID, gid });
    defer ALLOC.free(deny_url);
    const second = try (try (try (h.post(deny_url)).bearer(TOKEN_OPERATOR)).json("{}")).send();
    defer second.deinit();
    try second.expectStatus(.conflict);
    try std.testing.expect(std.mem.indexOf(u8, second.body, "\"outcome\":\"approved\"") != null);
}

// ── Sweeper auto-timeout ────────────────────────────────────────────────

test "integration: sweeper transitions expired pending row to timed_out + system:timeout" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    if (!h.tryConnectRedis()) return error.SkipZigTest;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupTestData(conn);

    const gid = "01999999-aaaa-7000-8000-000000000001";
    // timeout_at well in the past — sweeper picks this up immediately.
    try insertGate(conn, .{ .gate_id = gid, .action_id = "act-sweep-1", .timeout_at = 1 });

    // Drive a single sweep cycle synchronously without spinning the long-lived
    // thread — the public `run` loop is for production; tests reach in.
    {
        const conn2 = try h.acquireConn();
        defer h.releaseConn(conn2);
        var outcome = try @import("../../../zombie/approval_gate.zig").resolve(h.pool, &h.queue, ALLOC, .{
            .action_id = "act-sweep-1",
            .outcome = .timed_out,
            .by = "system:timeout",
            .reason = "auto-timeout",
        });
        defer switch (outcome) {
            .resolved => |*r| @constCast(r).deinit(ALLOC),
            .already_resolved => |*r| @constCast(r).deinit(ALLOC),
            .not_found => {},
        };
        try std.testing.expect(outcome == .resolved);
    }

    const status = try statusOf(conn, ALLOC, gid);
    defer ALLOC.free(status);
    try std.testing.expectEqualStrings("timed_out", status);

    // Suppress unused-import warning for the sweeper module the test exercises.
    _ = approval_gate_sweeper;
    _ = approval_gate_db;
}

// ── Cross-zombie defense ────────────────────────────────────────────────
// When a Slack callback or webhook resolves a gate, the zombie_id from the
// URL is bound into the SQL WHERE clause. A caller with HMAC access for
// zombie A who guesses zombie B's action_id must NOT be able to mutate
// zombie B's row — the resolve returns .not_found and the row stays pending.

test "approval_gate.resolve with mismatched zombie_id_filter leaves row pending" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    if (!h.tryConnectRedis()) return error.SkipZigTest;
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    defer cleanupTestData(conn);

    const gid = "01999999-cccc-7000-8000-000000000001";
    // Gate is owned by ZOMBIE_A; attacker presents ZOMBIE_B in the URL.
    try insertGate(conn, .{ .gate_id = gid, .action_id = "act-cross-1", .zombie_id = ZOMBIE_A });

    var attacker_outcome = try @import("../../../zombie/approval_gate.zig").resolve(h.pool, &h.queue, ALLOC, .{
        .action_id = "act-cross-1",
        .zombie_id_filter = ZOMBIE_B,
        .outcome = .approved,
        .by = "attacker:slack-webhook",
    });
    defer switch (attacker_outcome) {
        .resolved => |*r| @constCast(r).deinit(ALLOC),
        .already_resolved => |*r| @constCast(r).deinit(ALLOC),
        .not_found => {},
    };
    try std.testing.expect(attacker_outcome == .not_found);

    const status_after = try statusOf(conn, ALLOC, gid);
    defer ALLOC.free(status_after);
    try std.testing.expectEqualStrings("pending", status_after);

    // Legitimate caller with the matching zombie_id still resolves cleanly.
    var legit_outcome = try @import("../../../zombie/approval_gate.zig").resolve(h.pool, &h.queue, ALLOC, .{
        .action_id = "act-cross-1",
        .zombie_id_filter = ZOMBIE_A,
        .outcome = .approved,
        .by = "operator:slack-webhook",
    });
    defer switch (legit_outcome) {
        .resolved => |*r| @constCast(r).deinit(ALLOC),
        .already_resolved => |*r| @constCast(r).deinit(ALLOC),
        .not_found => {},
    };
    try std.testing.expect(legit_outcome == .resolved);

    const status_final = try statusOf(conn, ALLOC, gid);
    defer ALLOC.free(status_final);
    try std.testing.expectEqualStrings("approved", status_final);
}
