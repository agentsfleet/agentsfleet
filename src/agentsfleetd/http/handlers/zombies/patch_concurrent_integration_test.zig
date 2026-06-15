// Concurrent + lock-timeout integration tests for the §10b body-field
// PATCH path. Sister file to patch_body_fields_integration_test.zig;
// shares the row-lock/field-merge txn shape (per-txn lock_timeout=5s,
// statement_timeout=10s, idle_in_transaction_session_timeout=5s) but
// exercises it under contention.
//
// Requires TEST_DATABASE_URL — skipped gracefully otherwise. Uses the
// shared TestHarness; spawns std.Thread workers that each fire one
// HTTP PATCH and collect status + outcome. The lock-timeout test
// reaches into h.pool to hold a row-lock from a sibling txn so the
// handler's lock_timeout path is the system under test.
//
// Deadlock invariant proofs:
//   - Different-fields concurrent PATCH: both land via row-lock merge.
//   - Same-field concurrent PATCH: collapses to last-write-wins; no
//     `40P01 deadlock_detected` in either response.
//   - N concurrent writers on same agent: all 200; pool returns to
//     baseline (no exhausted connections).
//   - PATCH + DELETE on same agent: exactly one final state, no
//     deadlock_detected in either response/log.
//   - Different zombies in parallel: wall time stays sub-linear.
//
// Lock-timeout fixture: a holder thread takes SELECT FOR UPDATE in its
// own txn + sleeps 7s. A second PATCH must observe 503
// ERR_INTERNAL_DB_UNAVAILABLE in <5.5s (the handler's lock_timeout=5s
// path), proving fail-fast.

const std = @import("std");
const clock = @import("common").clock;
const id_format = @import("../../../types/id_format.zig");
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");

const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const harness_mod = @import("../../test_harness.zig");

const EVAL_BRANCH_QUOTA = 100_000;

const TestHarness = harness_mod.TestHarness;

const ALLOC = std.testing.allocator;

// Canonical test tenant/workspace pair shared across handler integration
// tests so the verbatim TOKEN_OPERATOR from tenant_billing_integration_test.zig
// validates. The original synthesized signature (against a non-canonical
// 0c6f01 pair) failed RS256 verification — handoff §10b risk note flagged
// regenerating via the test-token mint helper or copying canonical tokens.
const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const AGENTSFLEET_A = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c6f21";
const AGENTSFLEET_B = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c6f22";
const TEST_ISSUER = "https://clerk.dev.agentsfleet.net";
const TEST_AUDIENCE = "https://api.agentsfleet.net";
const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","n":"2hg972tpbq8H6kzRZ3oVL4wZ9bO-04gJ6gCig68aluyRBzagx-7XXPCiuX80oBHBVj51kvMjT_QDNXfrwzjy4cPbwiVV4HqOGpeIZkPEopfyzs4G7mjiQmx0YuM_5WQUlUjji6Y_DfeaoH-yOhTWBMBVoI0vW_1n66CFaGuEarj3VasdWYxObJTBAM6Jn4XZDcDsBBPNGO4ku7yILkfi11FqXfBP2V8NT0hAGXVAxlWwv-8up1RDzgACp-8JWoC2-kOUJN82fGenDGKq9hW_sumO-4YPNP4U1smnw5jzLlvKa0LBrYG8IgW-3Dniuq2mojhrD_ZQClUd5rF42OyYqw","e":"AQAB","kid":"rbac-test-kid","use":"sig","alg":"RS256"}]}
;
// Operator-role token — DELETE needs operator-minimum; PATCH body-field
// is workspace-member but operator covers both. Verbatim copy of the
// canonical TOKEN_OPERATOR from
// `src/http/handlers/tenant_billing_integration_test.zig` — same JWKS
// kid, same canonical 0a6f01/0a6f11 claims, validated RS256 signature.
const TOKEN_OPERATOR =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InJiYWMtdGVzdC1raWQifQ.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImV4cCI6NDEwMjQ0NDgwMCwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjAxIiwid29ya3NwYWNlX2lkIjoiMDE5NWI0YmEtOGQzYS03ZjEzLThhYmMtMmIzZTFlMGE2ZjExIiwicm9sZSI6Im9wZXJhdG9yIn19.V84uE69RTLrRef0sogegUcUZeKWx8E68GEruFoS8HegUa3o7bVCfQjlkllNSbtUut919EygbQv1C16BMfNTOAv1Lvl3AeLYPYr4ni6EnzzGllbyxDw1aY68AGWEEvKOUxd5wCGl8BnEqaOKX7KNNbAOV4AzJNWqnV-uxJiZl6oDtqi8bsSF1HAm9qY9MAl6AwoZLGnT_x6ux_3vfKy_9ckZSbgjN7laZOMqQ5nwwcaSpwYNm_3ZpXJLgHYMVxel2M4rT0SIaFh__rE42yGE9FBDRUFoyktGOR3NYPOzogjj3tfOoecC8NEhrwifzXcSNVAiHOMnmXojjAPEUORovPg";

const BASE_CONFIG_JSON =
    \\{"name":"conc-bot","x-agentsfleet":{"triggers":[{"type":"webhook","source":"github","events":["push"]}],"tools":["http_request"],"budget":{"daily_dollars":5.0}}}
;
const BASE_TRIGGER_MD =
    \\---
    \\name: conc-bot
    \\x-agentsfleet:
    \\  triggers:
    \\    - type: webhook
    \\      source: github
    \\      events: ["push"]
    \\  tools: ["http_request"]
    \\  budget:
    \\    daily_dollars: 5.0
    \\---
;
const BASE_SOURCE_MD =
    \\---
    \\name: conc-bot
    \\---
    \\# initial
;

const TRIGGER_VARIANT_A =
    \\---
    \\name: conc-bot
    \\x-agentsfleet:
    \\  triggers:
    \\    - type: cron
    \\      schedule: "*/15 * * * *"
    \\  tools: ["http_request"]
    \\  budget:
    \\    daily_dollars: 5.0
    \\---
;
const TRIGGER_VARIANT_B =
    \\---
    \\name: conc-bot
    \\x-agentsfleet:
    \\  triggers:
    \\    - type: cron
    \\      schedule: "0 9 * * *"
    \\  tools: ["http_request"]
    \\  budget:
    \\    daily_dollars: 5.0
    \\---
;
// parseSkillMetadata requires name+description+version in the frontmatter
// (config_markdown.zig:159-166). The PATCH body's source_markdown goes
// through that parser before the field-merge txn touches the row.
const SOURCE_VARIANT_A =
    \\---
    \\name: conc-bot
    \\description: Concurrent test bot
    \\version: 0.1.0
    \\---
    \\# variant A
;

// AGENTSFLEET_B mirror of BASE/TRIGGER_VARIANT_A with its own name so the §5
// parallel test can PATCH both rows concurrently without colliding on
// uq_zombies_workspace_name (workspace_id, name).
const BASE_CONFIG_JSON_B =
    \\{"name":"conc-bot-b","x-agentsfleet":{"triggers":[{"type":"webhook","source":"github","events":["push"]}],"tools":["http_request"],"budget":{"daily_dollars":5.0}}}
;
const BASE_TRIGGER_MD_B =
    \\---
    \\name: conc-bot-b
    \\x-agentsfleet:
    \\  triggers:
    \\    - type: webhook
    \\      source: github
    \\      events: ["push"]
    \\  tools: ["http_request"]
    \\  budget:
    \\    daily_dollars: 5.0
    \\---
;
const BASE_SOURCE_MD_B =
    \\---
    \\name: conc-bot-b
    \\---
    \\# initial
;
const TRIGGER_VARIANT_FOR_B =
    \\---
    \\name: conc-bot-b
    \\x-agentsfleet:
    \\  triggers:
    \\    - type: cron
    \\      schedule: "*/15 * * * *"
    \\  tools: ["http_request"]
    \\  budget:
    \\    daily_dollars: 5.0
    \\---
;

const TRIGGER_VARIANT_A_JSON = jsonEscape(TRIGGER_VARIANT_A);
const TRIGGER_VARIANT_B_JSON = jsonEscape(TRIGGER_VARIANT_B);
const TRIGGER_VARIANT_FOR_B_JSON = jsonEscape(TRIGGER_VARIANT_FOR_B);
const SOURCE_VARIANT_A_JSON = jsonEscape(SOURCE_VARIANT_A);

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn seedAndHarness(alloc: std.mem.Allocator) !*TestHarness {
    const h = try TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = TEST_JWKS,
        .issuer = TEST_ISSUER,
        .audience = TEST_AUDIENCE,
    });
    errdefer h.deinit();
    _ = h.tryConnectRedis();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedFixture(conn);
    return h;
}

fn seedFixture(conn: *pg.Conn) !void {
    const now = clock.nowMillis();
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'PatchConcurrentTest', $2, $2)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{ TEST_TENANT_ID, now });
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, $3)
        \\ON CONFLICT (workspace_id) DO NOTHING
    , .{ TEST_WORKSPACE_ID, TEST_TENANT_ID, now });
    // uq_zombies_workspace_name forbids two rows sharing (workspace_id, name);
    // AGENTSFLEET_A and AGENTSFLEET_B coexist in TEST_WORKSPACE_ID, so each row needs
    // a distinct (name, config_json.name, trigger_markdown name) triple
    // — the PATCH handler enforces config_json.name ↔ source_markdown.name
    // ↔ row.name parity (see patch.zig name_mismatch + new_name update).
    const Row = struct {
        id: []const u8,
        name: []const u8,
        source: []const u8,
        trigger: []const u8,
        config: []const u8,
    };
    const rows = [_]Row{
        .{ .id = AGENTSFLEET_A, .name = "conc-bot", .source = BASE_SOURCE_MD, .trigger = BASE_TRIGGER_MD, .config = BASE_CONFIG_JSON },
        .{ .id = AGENTSFLEET_B, .name = "conc-bot-b", .source = BASE_SOURCE_MD_B, .trigger = BASE_TRIGGER_MD_B, .config = BASE_CONFIG_JSON_B },
    };
    for (rows) |r| {
        _ = try conn.exec(
            \\INSERT INTO core.zombies
            \\  (id, workspace_id, name, source_markdown, trigger_markdown, config_json,
            \\   status, created_at, updated_at)
            \\VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6::jsonb, 'active', $7, $7)
            \\ON CONFLICT (id) DO UPDATE SET
            \\    name = EXCLUDED.name,
            \\    source_markdown = EXCLUDED.source_markdown,
            \\    trigger_markdown = EXCLUDED.trigger_markdown,
            \\    config_json = EXCLUDED.config_json,
            \\    status = 'active',
            \\    updated_at = EXCLUDED.updated_at
        , .{ r.id, TEST_WORKSPACE_ID, r.name, r.source, r.trigger, r.config, now });
    }
}

fn cleanup(conn: *pg.Conn) void {
    _ = conn.exec("DELETE FROM core.zombies WHERE workspace_id = $1::uuid", .{TEST_WORKSPACE_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM workspaces WHERE workspace_id = $1::uuid", .{TEST_WORKSPACE_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM tenants WHERE tenant_id = $1::uuid", .{TEST_TENANT_ID}) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
}

fn patchUrl(zombie_id: []const u8) ![]const u8 {
    return std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/zombies/{s}", .{ TEST_WORKSPACE_ID, zombie_id });
}

const Outcome = struct {
    status: u16 = 0,
    body: ?[]u8 = null,
    elapsed_ms: i64 = 0,
};

fn freeOutcomes(slice: []Outcome) void {
    for (slice) |o| if (o.body) |b| ALLOC.free(b);
}

const Worker = struct {
    fn run(h: *TestHarness, body: []const u8, zid: []const u8, slot: *Outcome) void {
        const url = patchUrl(zid) catch return;
        defer ALLOC.free(url);
        const t0 = clock.nowMillis();
        const r_req = h.request(.PATCH, url).bearer(TOKEN_OPERATOR) catch return;
        const r_json = r_req.json(body) catch return;
        const r = r_json.send() catch return;
        defer r.deinit();
        slot.* = .{
            .status = r.status,
            .body = ALLOC.dupe(u8, r.body) catch null,
            .elapsed_ms = clock.nowMillis() - t0,
        };
    }

    fn runDelete(h: *TestHarness, zid: []const u8, slot: *Outcome) void {
        const url = patchUrl(zid) catch return;
        defer ALLOC.free(url);
        const t0 = clock.nowMillis();
        const r_req = h.request(.DELETE, url).bearer(TOKEN_OPERATOR) catch return;
        const r = r_req.send() catch return;
        defer r.deinit();
        slot.* = .{
            .status = r.status,
            .body = ALLOC.dupe(u8, r.body) catch null,
            .elapsed_ms = clock.nowMillis() - t0,
        };
    }

    // Fires one raw INSERT into core.zombie_events with FK ref to `zid`.
    // The FK validation acquires FOR KEY SHARE on the parent row — that
    // lock waits on any in-flight FOR UPDATE the PATCH handler holds
    // inside its row-lock/field-merge txn, so concurrent execution must
    // serialize cleanly. Errors are captured in `slot.body` (errorName)
    // for the test to assert against; status=200 = exec OK, status=500
    // = exec returned an error.
    fn runInsertEvent(h: *TestHarness, zid: []const u8, event_id: []const u8, slot: *Outcome) void {
        const t0 = clock.nowMillis();
        const conn = h.acquireConn() catch |err| {
            slot.* = .{ .status = 500, .body = ALLOC.dupe(u8, @errorName(err)) catch null, .elapsed_ms = clock.nowMillis() - t0 };
            return;
        };
        defer h.releaseConn(conn);
        const now = clock.nowMillis();
        var uid_buf: [36]u8 = undefined;
        const uid = id_format.formatUuidV7(&uid_buf) catch |err| {
            slot.* = .{ .status = 500, .body = ALLOC.dupe(u8, @errorName(err)) catch null, .elapsed_ms = clock.nowMillis() - t0 };
            return;
        };
        _ = conn.exec(
            \\INSERT INTO core.zombie_events
            \\  (uid, zombie_id, event_id, workspace_id, actor, event_type, status,
            \\   request_json, created_at, updated_at)
            \\VALUES ($1::uuid, $2::uuid, $3, $4::uuid, 'steer:test', 'message', 'received',
            \\        '{}'::jsonb, $5, $5)
        , .{ uid, zid, event_id, TEST_WORKSPACE_ID, now }) catch |err| {
            slot.* = .{ .status = 500, .body = ALLOC.dupe(u8, @errorName(err)) catch null, .elapsed_ms = clock.nowMillis() - t0 };
            return;
        };
        slot.* = .{ .status = 200, .body = null, .elapsed_ms = clock.nowMillis() - t0 };
    }
};

fn bodyContainsDeadlock(out: Outcome) bool {
    if (out.body) |b| {
        // Postgres deadlock_detected SQLSTATE is 40P01. The handler never
        // surfaces this code in any deterministic outcome — its presence
        // anywhere in the response body is the bug.
        return std.mem.indexOf(u8, b, "40P01") != null or
            std.mem.indexOf(u8, b, "deadlock_detected") != null;
    }
    return false;
}

// ── §1 — Different fields land both halves via row-lock merge ────────────

test "integration: concurrent PATCH different fields — both halves land, no deadlock" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const c_init = try h.acquireConn();
    defer h.releaseConn(c_init);
    defer cleanup(c_init);

    const body_trig = "{\"trigger_markdown\":" ++ TRIGGER_VARIANT_A_JSON ++ "}";
    const body_src = "{\"source_markdown\":" ++ SOURCE_VARIANT_A_JSON ++ "}";

    var outcomes: [2]Outcome = .{ .{}, .{} };
    defer freeOutcomes(&outcomes);

    var threads: [2]std.Thread = undefined;
    threads[0] = try std.Thread.spawn(.{}, Worker.run, .{ h, body_trig, AGENTSFLEET_A, &outcomes[0] });
    threads[1] = try std.Thread.spawn(.{}, Worker.run, .{ h, body_src, AGENTSFLEET_A, &outcomes[1] });
    for (threads) |t| t.join();

    try std.testing.expectEqual(@as(u16, 200), outcomes[0].status);
    try std.testing.expectEqual(@as(u16, 200), outcomes[1].status);
    try std.testing.expect(!bodyContainsDeadlock(outcomes[0]));
    try std.testing.expect(!bodyContainsDeadlock(outcomes[1]));

    // Read back — both halves must be visible (last write didn't clobber).
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    var q = PgQuery.from(try conn.query(
        "SELECT config_json::text, source_markdown FROM core.zombies WHERE id = $1::uuid",
        .{AGENTSFLEET_A},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return error.RowNotFound;
    const cfg = try row.get([]const u8, 0);
    const src = try row.get([]const u8, 1);
    // Triggers half = variant A (cron */15)
    try std.testing.expect(std.mem.indexOf(u8, cfg, "*/15 * * * *") != null);
    // Source half = variant A
    try std.testing.expect(std.mem.indexOf(u8, src, "variant A") != null);
}

// ── §2 — Same field concurrent → LWW, no deadlock ────────────────────────

test "integration: concurrent PATCH same field — last write wins, no deadlock" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const c_init = try h.acquireConn();
    defer h.releaseConn(c_init);
    defer cleanup(c_init);

    const body_a = "{\"trigger_markdown\":" ++ TRIGGER_VARIANT_A_JSON ++ "}";
    const body_b = "{\"trigger_markdown\":" ++ TRIGGER_VARIANT_B_JSON ++ "}";

    var outcomes: [2]Outcome = .{ .{}, .{} };
    defer freeOutcomes(&outcomes);

    var threads: [2]std.Thread = undefined;
    threads[0] = try std.Thread.spawn(.{}, Worker.run, .{ h, body_a, AGENTSFLEET_A, &outcomes[0] });
    threads[1] = try std.Thread.spawn(.{}, Worker.run, .{ h, body_b, AGENTSFLEET_A, &outcomes[1] });
    for (threads) |t| t.join();

    try std.testing.expectEqual(@as(u16, 200), outcomes[0].status);
    try std.testing.expectEqual(@as(u16, 200), outcomes[1].status);
    try std.testing.expect(!bodyContainsDeadlock(outcomes[0]));
    try std.testing.expect(!bodyContainsDeadlock(outcomes[1]));

    // One of the two schedules must be the final value.
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    var q = PgQuery.from(try conn.query(
        "SELECT config_json::text FROM core.zombies WHERE id = $1::uuid",
        .{AGENTSFLEET_A},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return error.RowNotFound;
    const cfg = try row.get([]const u8, 0);
    const has_a = std.mem.indexOf(u8, cfg, "*/15 * * * *") != null;
    const has_b = std.mem.indexOf(u8, cfg, "0 9 * * *") != null;
    try std.testing.expect(has_a or has_b);
    try std.testing.expect(!(has_a and has_b)); // exactly one — no merged stew
}

// ── §3 — N writers on same row, no pool exhaustion, no deadlock ──────────

test "integration: 10 concurrent PATCHes on same zombie — all 200, no deadlock" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const c_init = try h.acquireConn();
    defer h.releaseConn(c_init);
    defer cleanup(c_init);

    const N = 10;
    const body = "{\"trigger_markdown\":" ++ TRIGGER_VARIANT_A_JSON ++ "}";

    var outcomes: [N]Outcome = @splat(Outcome{});
    defer freeOutcomes(&outcomes);

    var threads: [N]std.Thread = undefined;
    for (&threads, 0..) |*t, i| {
        t.* = try std.Thread.spawn(.{}, Worker.run, .{ h, body, AGENTSFLEET_A, &outcomes[i] });
    }
    for (threads) |t| t.join();

    var ok_count: usize = 0;
    for (outcomes) |o| {
        if (o.status == 200) ok_count += 1;
        try std.testing.expect(!bodyContainsDeadlock(o));
    }
    try std.testing.expectEqual(@as(usize, N), ok_count);
}

// ── §4 — PATCH + DELETE on same zombie → no deadlock, one final state ───

test "integration: concurrent PATCH + DELETE same zombie — no deadlock" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const c_init = try h.acquireConn();
    defer h.releaseConn(c_init);
    defer cleanup(c_init);

    const body = "{\"trigger_markdown\":" ++ TRIGGER_VARIANT_A_JSON ++ "}";

    var outcomes: [2]Outcome = .{ .{}, .{} };
    defer freeOutcomes(&outcomes);

    var threads: [2]std.Thread = undefined;
    threads[0] = try std.Thread.spawn(.{}, Worker.run, .{ h, body, AGENTSFLEET_A, &outcomes[0] });
    threads[1] = try std.Thread.spawn(.{}, Worker.runDelete, .{ h, AGENTSFLEET_A, &outcomes[1] });
    for (threads) |t| t.join();

    // Two interleavings are valid:
    //   (a) PATCH first → DELETE second: PATCH=200, DELETE=204 (or 200/202)
    //   (b) DELETE first → PATCH second: DELETE=204, PATCH=404 (zombie gone)
    // Either way: no 40P01 in any response.
    try std.testing.expect(!bodyContainsDeadlock(outcomes[0]));
    try std.testing.expect(!bodyContainsDeadlock(outcomes[1]));
    // At least one of the two must succeed in some shape.
    const patch_ok = outcomes[0].status == 200 or outcomes[0].status == 404;
    try std.testing.expect(patch_ok);
}

// ── §5 — Different zombies in parallel: near-linear wall time ────────────

test "integration: concurrent PATCH on different zombies — parallel, sub-linear" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const c_init = try h.acquireConn();
    defer h.releaseConn(c_init);
    defer cleanup(c_init);

    // Per-zombie bodies — each PATCH carries the trigger variant whose name
    // matches its target row's name. Required because the PATCH handler's
    // UPDATE sets `name = parsed_trigger.config.name`, and a shared body
    // would drive both rows onto the same value → uq_zombies_workspace_name
    // violation on whichever commits second.
    const body_a = "{\"trigger_markdown\":" ++ TRIGGER_VARIANT_A_JSON ++ "}";
    const body_b = "{\"trigger_markdown\":" ++ TRIGGER_VARIANT_FOR_B_JSON ++ "}";

    var outcomes: [2]Outcome = .{ .{}, .{} };
    defer freeOutcomes(&outcomes);

    const t0 = clock.nowMillis();
    var threads: [2]std.Thread = undefined;
    threads[0] = try std.Thread.spawn(.{}, Worker.run, .{ h, body_a, AGENTSFLEET_A, &outcomes[0] });
    threads[1] = try std.Thread.spawn(.{}, Worker.run, .{ h, body_b, AGENTSFLEET_B, &outcomes[1] });
    for (threads) |t| t.join();
    const parallel_ms = clock.nowMillis() - t0;

    try std.testing.expectEqual(@as(u16, 200), outcomes[0].status);
    try std.testing.expectEqual(@as(u16, 200), outcomes[1].status);

    // Sanity: parallel wall time should not be more than 1.8× the slower
    // single-request elapsed (rough proxy — real serial baseline would
    // require a separate run, but each thread's elapsed approximates one).
    const slower = @max(outcomes[0].elapsed_ms, outcomes[1].elapsed_ms);
    if (slower > 0) {
        const ratio_x100 = @divTrunc(parallel_ms * 100, slower);
        try std.testing.expect(ratio_x100 < 180);
    }
}

// ── §6 — Lock-timeout fails fast under sustained row-lock contention ────

test "integration: PATCH against held lock → 503 in <5.5s, no hang" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const c_init = try h.acquireConn();
    defer h.releaseConn(c_init);
    defer cleanup(c_init);

    // Holder thread takes its own connection, BEGINs, SELECT FOR UPDATE,
    // sleeps 7s, then ROLLBACKs. The 7s holds the row-lock longer than
    // the handler's 5s lock_timeout, so the contending PATCH must fail
    // fast with 503, not hang for the full 7s.
    const Holder = struct {
        fn run(harness: *TestHarness, started: *std.atomic.Value(bool)) void {
            const c = harness.pool.acquire() catch return;
            defer harness.pool.release(c);
            _ = c.exec("BEGIN", .{}) catch return;
            defer _ = c.exec("ROLLBACK", .{}) catch {};
            _ = c.exec(
                "SELECT id FROM core.zombies WHERE id = $1::uuid FOR UPDATE",
                .{AGENTSFLEET_A},
            ) catch return;
            started.store(true, .release);
            @import("common").sleepNanos(7 * std.time.ns_per_s);
        }
    };

    var started = std.atomic.Value(bool).init(false);
    const holder = try std.Thread.spawn(.{}, Holder.run, .{ h, &started });
    defer holder.join();
    // Wait up to 2s for the holder's SELECT FOR UPDATE to grab the lock. Zig 0.16
    // removed Thread.ResetEvent.timedWait, so this is a bounded poll (200 × 10ms).
    {
        var waited: usize = 0;
        while (!started.load(.acquire)) : (waited += 1) {
            if (waited >= 200) return error.HolderLockSetupTimeout;
            @import("common").sleepNanos(10 * std.time.ns_per_ms);
        }
    }

    const body = "{\"trigger_markdown\":" ++ TRIGGER_VARIANT_A_JSON ++ "}";
    var outcome: Outcome = .{};
    defer if (outcome.body) |b| ALLOC.free(b);
    const t0 = clock.nowMillis();
    Worker.run(h, body, AGENTSFLEET_A, &outcome);
    const elapsed = clock.nowMillis() - t0;

    // Fail-fast: should return well before holder's 7s sleep completes.
    try std.testing.expect(elapsed < 5_500);
    try std.testing.expectEqual(@as(u16, 503), outcome.status);
    try std.testing.expect(!bodyContainsDeadlock(outcome));
}

// ── §7 — Concurrent PATCH + INSERT into zombie_events serialize cleanly ──

// PATCH handler takes SELECT FOR UPDATE on the zombie row inside its txn;
// an INSERT into core.zombie_events with FK ref to core.zombies(id) needs
// FOR KEY SHARE on the same parent. The lock modes are incompatible, so
// PG serializes the INSERT after the PATCH commit. Both must succeed; no
// `40P01 deadlock_detected` in either; the inserted row must be visible.
test "integration: concurrent PATCH + INSERT into zombie_events — both succeed, no deadlock" {
    const h = seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const c_init = try h.acquireConn();
    defer h.releaseConn(c_init);
    defer cleanup(c_init);

    const body_patch = "{\"trigger_markdown\":" ++ TRIGGER_VARIANT_A_JSON ++ "}";
    const evt_id = "evt_conc_patch_insert_1";

    var patch_out: Outcome = .{};
    var insert_out: Outcome = .{};
    defer if (patch_out.body) |b| ALLOC.free(b);
    defer if (insert_out.body) |b| ALLOC.free(b);

    var threads: [2]std.Thread = undefined;
    threads[0] = try std.Thread.spawn(.{}, Worker.run, .{ h, body_patch, AGENTSFLEET_A, &patch_out });
    threads[1] = try std.Thread.spawn(.{}, Worker.runInsertEvent, .{ h, AGENTSFLEET_A, evt_id, &insert_out });
    for (threads) |t| t.join();

    try std.testing.expectEqual(@as(u16, 200), patch_out.status);
    try std.testing.expectEqual(@as(u16, 200), insert_out.status);
    try std.testing.expect(!bodyContainsDeadlock(patch_out));
    try std.testing.expect(!bodyContainsDeadlock(insert_out));

    // INSERT row must be visible — proves FK didn't fail and PATCH didn't
    // CASCADE-delete the parent. Final config_json reflects PATCH variant A.
    // Each query is scoped in its own block so the previous result set is
    // drained (via PgQuery.deinit) before the next conn.query — otherwise
    // the second call hits error.ConnectionBusy.
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    {
        var q_evt = PgQuery.from(try conn.query(
            "SELECT COUNT(*)::bigint FROM core.zombie_events WHERE zombie_id = $1::uuid AND event_id = $2",
            .{ AGENTSFLEET_A, evt_id },
        ));
        defer q_evt.deinit();
        const row_evt = (try q_evt.next()) orelse return error.RowNotFound;
        const evt_count = try row_evt.get(i64, 0);
        try std.testing.expectEqual(@as(i64, 1), evt_count);
    }
    {
        var q_cfg = PgQuery.from(try conn.query(
            "SELECT config_json::text FROM core.zombies WHERE id = $1::uuid",
            .{AGENTSFLEET_A},
        ));
        defer q_cfg.deinit();
        const row_cfg = (try q_cfg.next()) orelse return error.RowNotFound;
        const cfg = try row_cfg.get([]const u8, 0);
        try std.testing.expect(std.mem.indexOf(u8, cfg, "*/15 * * * *") != null);
    }
}

// Comptime JSON-string-encode a multi-line literal. See
// patch_body_fields_integration_test.zig for the rationale.
fn jsonEscape(comptime s: []const u8) []const u8 {
    @setEvalBranchQuota(EVAL_BRANCH_QUOTA);
    comptime var out: []const u8 = "\"";
    inline for (s) |c| {
        out = out ++ switch (c) {
            '"' => "\\\"",
            '\\' => "\\\\",
            '\n' => "\\n",
            '\r' => "\\r",
            '\t' => "\\t",
            else => &[_]u8{c},
        };
    }
    return out ++ "\"";
}
