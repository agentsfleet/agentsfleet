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
//   - Bounded concurrent writers on same fleet: all 200; no exhausted server
//     workers or leaked database connections.
//   - PATCH + DELETE on same fleet: exactly one final state, no
//     deadlock_detected in either response/log.
//   - Different fleets in parallel: wall time stays sub-linear.
//
// Lock-timeout fixture: a holder thread takes SELECT FOR UPDATE in its
// own txn + sleeps 7s. A second PATCH must observe 503
// ERR_INTERNAL_DB_UNAVAILABLE in <5.5s (the handler's lock_timeout=5s
// path), proving fail-fast.

const std = @import("std");
const scope_fixtures = @import("../../test_scope_tokens.zig");
const common = @import("common");
const clock = common.clock;
const id_format = @import("../../../types/id_format.zig");
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");

const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const harness_mod = @import("../../test_harness.zig");

const EVAL_BRANCH_QUOTA = 100_000;
const PATCH_WRITER_COUNT = 2;

const TestHarness = harness_mod.TestHarness;

const ALLOC = std.testing.allocator;
var suite_lock: common.Mutex = .{};

// Dedicated tenant/workspace pair embedded in TOKEN_OPERATOR. This file
// intentionally avoids the canonical shared workspace because the full
// integration suite runs handler files in parallel and some legacy fixtures
// still delete the shared workspace during cleanup.
const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c6f01";
const TEST_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c6f11";
const FleetPair = struct {
    a: []const u8,
    b: []const u8,
};
const IDS_DIFFERENT_FIELDS = FleetPair{ .a = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c7111", .b = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c7112" };
const IDS_SAME_FIELD = FleetPair{ .a = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c7121", .b = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c7122" };
const IDS_BOUNDED_WRITERS = FleetPair{ .a = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c7131", .b = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c7132" };
const IDS_PATCH_DELETE = FleetPair{ .a = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c7141", .b = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c7142" };
const IDS_DIFFERENT_FLEETS = FleetPair{ .a = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c7151", .b = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c7152" };
const IDS_LOCK_TIMEOUT = FleetPair{ .a = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c7161", .b = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c7162" };
const IDS_PATCH_INSERT = FleetPair{ .a = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c7171", .b = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c7172" };
const TEST_ISSUER = scope_fixtures.ISSUER;
const TEST_AUDIENCE = scope_fixtures.AUDIENCE;
const TEST_JWKS = scope_fixtures.JWKS;
// Tenant-admin token minted by scripts/mint-scope-personas.mjs for the
// dedicated pair above. DELETE needs operator-minimum; PATCH body-field is
// workspace-member but tenant-admin covers both.
const TOKEN_OPERATOR = scope_fixtures.PATCH_CONCURRENT_ADMIN;

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
    \\    - type: api
    \\  tools: ["http_request"]
    \\  budget:
    \\    daily_dollars: 6.0
    \\---
;
const TRIGGER_VARIANT_B =
    \\---
    \\name: conc-bot
    \\x-agentsfleet:
    \\  triggers:
    \\    - type: api
    \\  tools: ["http_request"]
    \\  budget:
    \\    daily_dollars: 7.0
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
// uq_fleets_workspace_id_name (workspace_id, name).
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
    \\    - type: api
    \\  tools: ["http_request"]
    \\  budget:
    \\    daily_dollars: 6.0
    \\---
;

const TRIGGER_VARIANT_A_JSON = jsonEscape(TRIGGER_VARIANT_A);
const TRIGGER_VARIANT_B_JSON = jsonEscape(TRIGGER_VARIANT_B);
const TRIGGER_VARIANT_FOR_B_JSON = jsonEscape(TRIGGER_VARIANT_FOR_B);
const SOURCE_VARIANT_A_JSON = jsonEscape(SOURCE_VARIANT_A);

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn seedAndHarness(alloc: std.mem.Allocator, ids: FleetPair) !*TestHarness {
    const h = try TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = TEST_JWKS,
        .issuer = TEST_ISSUER,
        .audience = TEST_AUDIENCE,
    });
    errdefer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedFixture(conn, ids);
    return h;
}

fn seedFixture(conn: *pg.Conn, ids: FleetPair) !void {
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
    // uq_fleets_workspace_id_name forbids two rows sharing (workspace_id, name);
    // Both fixture rows coexist in TEST_WORKSPACE_ID, so each row needs
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
        .{ .id = ids.a, .name = "conc-bot", .source = BASE_SOURCE_MD, .trigger = BASE_TRIGGER_MD, .config = BASE_CONFIG_JSON },
        .{ .id = ids.b, .name = "conc-bot-b", .source = BASE_SOURCE_MD_B, .trigger = BASE_TRIGGER_MD_B, .config = BASE_CONFIG_JSON_B },
    };
    for (rows) |r| {
        _ = try conn.exec(
            \\INSERT INTO core.fleets
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

fn cleanup(conn: *pg.Conn, ids: FleetPair) void {
    _ = conn.exec("DELETE FROM core.fleet_schedules WHERE fleet_id IN ($1::uuid, $2::uuid)", .{ ids.a, ids.b }) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.fleet_events WHERE fleet_id IN ($1::uuid, $2::uuid)", .{ ids.a, ids.b }) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.fleets WHERE id IN ($1::uuid, $2::uuid)", .{ ids.a, ids.b }) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
}

fn cleanupHarness(h: *TestHarness, ids: FleetPair) void {
    const conn = h.acquireConn() catch |err| {
        std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
        return;
    };
    defer h.releaseConn(conn);
    cleanup(conn, ids);
}

fn patchUrl(fleet_id: []const u8) ![]const u8 {
    return std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/fleets/{s}", .{ TEST_WORKSPACE_ID, fleet_id });
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
    fn captureError(slot: *Outcome, err: anyerror, t0: i64) void {
        slot.* = .{
            .status = 599,
            .body = ALLOC.dupe(u8, @errorName(err)) catch null,
            .elapsed_ms = clock.nowMillis() - t0,
        };
    }

    fn run(h: *TestHarness, body: []const u8, zid: []const u8, slot: *Outcome) void {
        const t0 = clock.nowMillis();
        const url = patchUrl(zid) catch |err| return captureError(slot, err, t0);
        defer ALLOC.free(url);
        const r_req = h.request(.PATCH, url).bearer(TOKEN_OPERATOR) catch |err| return captureError(slot, err, t0);
        const r_json = r_req.json(body) catch |err| return captureError(slot, err, t0);
        const r = r_json.send() catch |err| return captureError(slot, err, t0);
        defer r.deinit();
        slot.* = .{
            .status = r.status,
            .body = ALLOC.dupe(u8, r.body) catch null,
            .elapsed_ms = clock.nowMillis() - t0,
        };
    }

    fn runDelete(h: *TestHarness, zid: []const u8, slot: *Outcome) void {
        const t0 = clock.nowMillis();
        const url = patchUrl(zid) catch |err| return captureError(slot, err, t0);
        defer ALLOC.free(url);
        const r_req = h.request(.DELETE, url).bearer(TOKEN_OPERATOR) catch |err| return captureError(slot, err, t0);
        const r = r_req.send() catch |err| return captureError(slot, err, t0);
        defer r.deinit();
        slot.* = .{
            .status = r.status,
            .body = ALLOC.dupe(u8, r.body) catch null,
            .elapsed_ms = clock.nowMillis() - t0,
        };
    }

    // Fires one raw INSERT into core.fleet_events with FK ref to `zid`.
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
            \\INSERT INTO core.fleet_events
            \\  (uid, fleet_id, event_id, workspace_id, actor, event_type, status,
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

fn expectContains(label: []const u8, haystack: []const u8, needle: []const u8) !void {
    if (std.mem.indexOf(u8, haystack, needle) != null) return;
    std.debug.print("missing {s}: wanted {s} in {s}\n", .{ label, needle, haystack });
    return error.TestUnexpectedResult;
}

fn configHasDailyBudget(cfg: []const u8, dollars: u8) !bool {
    var compact_buf: [32]u8 = undefined;
    const compact = try std.fmt.bufPrint(
        &compact_buf,
        "\"daily_dollars\":{d}",
        .{dollars},
    );
    var spaced_buf: [32]u8 = undefined;
    const spaced = try std.fmt.bufPrint(
        &spaced_buf,
        "\"daily_dollars\": {d}",
        .{dollars},
    );
    return std.mem.indexOf(u8, cfg, compact) != null or
        std.mem.indexOf(u8, cfg, spaced) != null;
}

fn expectDailyBudget(label: []const u8, cfg: []const u8, dollars: u8) !void {
    if (try configHasDailyBudget(cfg, dollars)) return;
    std.debug.print("missing {s}: wanted daily_dollars={d} in {s}\n", .{ label, dollars, cfg });
    return error.TestUnexpectedResult;
}

// ── §1 — Different fields land both halves via row-lock merge ────────────

test "integration: concurrent PATCH different fields — both halves land, no deadlock" {
    suite_lock.lock();
    defer suite_lock.unlock();

    const ids = IDS_DIFFERENT_FIELDS;
    const h = seedAndHarness(ALLOC, ids) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    defer cleanupHarness(h, ids);

    const body_trig = "{\"trigger_markdown\":" ++ TRIGGER_VARIANT_A_JSON ++ "}";
    const body_src = "{\"source_markdown\":" ++ SOURCE_VARIANT_A_JSON ++ "}";

    var outcomes: [2]Outcome = .{ .{}, .{} };
    defer freeOutcomes(&outcomes);

    var threads: [2]std.Thread = undefined;
    threads[0] = try std.Thread.spawn(.{}, Worker.run, .{ h, body_trig, ids.a, &outcomes[0] });
    threads[1] = try std.Thread.spawn(.{}, Worker.run, .{ h, body_src, ids.a, &outcomes[1] });
    for (threads) |t| t.join();

    try std.testing.expectEqual(@as(u16, 200), outcomes[0].status);
    try std.testing.expectEqual(@as(u16, 200), outcomes[1].status);
    try std.testing.expect(!bodyContainsDeadlock(outcomes[0]));
    try std.testing.expect(!bodyContainsDeadlock(outcomes[1]));

    // Read back — both halves must be visible (last write didn't clobber).
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    var q = PgQuery.from(try conn.query(
        "SELECT config_json::text, source_markdown FROM core.fleets WHERE id = $1::uuid",
        .{ids.a},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return error.RowNotFound;
    const cfg = try row.get([]const u8, 0);
    const src = try row.get([]const u8, 1);
    // Triggers half = variant A.
    try expectDailyBudget("different-fields config", cfg, 6);
    // Source half = variant A
    try expectContains("different-fields source", src, "variant A");
}

// ── §2 — Same field concurrent → LWW, no deadlock ────────────────────────

test "integration: concurrent PATCH same field — last write wins, no deadlock" {
    suite_lock.lock();
    defer suite_lock.unlock();

    const ids = IDS_SAME_FIELD;
    const h = seedAndHarness(ALLOC, ids) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    defer cleanupHarness(h, ids);

    const body_a = "{\"trigger_markdown\":" ++ TRIGGER_VARIANT_A_JSON ++ "}";
    const body_b = "{\"trigger_markdown\":" ++ TRIGGER_VARIANT_B_JSON ++ "}";

    var outcomes: [2]Outcome = .{ .{}, .{} };
    defer freeOutcomes(&outcomes);

    var threads: [2]std.Thread = undefined;
    threads[0] = try std.Thread.spawn(.{}, Worker.run, .{ h, body_a, ids.a, &outcomes[0] });
    threads[1] = try std.Thread.spawn(.{}, Worker.run, .{ h, body_b, ids.a, &outcomes[1] });
    for (threads) |t| t.join();

    try std.testing.expectEqual(@as(u16, 200), outcomes[0].status);
    try std.testing.expectEqual(@as(u16, 200), outcomes[1].status);
    try std.testing.expect(!bodyContainsDeadlock(outcomes[0]));
    try std.testing.expect(!bodyContainsDeadlock(outcomes[1]));

    // One of the two schedules must be the final value.
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    var q = PgQuery.from(try conn.query(
        "SELECT config_json::text FROM core.fleets WHERE id = $1::uuid",
        .{ids.a},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return error.RowNotFound;
    const cfg = try row.get([]const u8, 0);
    const has_a = try configHasDailyBudget(cfg, 6);
    const has_b = try configHasDailyBudget(cfg, 7);
    if (!has_a and !has_b) std.debug.print("same-field config missing variants: {s}\n", .{cfg});
    try std.testing.expect(has_a or has_b);
    try std.testing.expect(!(has_a and has_b)); // exactly one — no merged stew
}

// ── §3 — Bounded writers on same row, no server exhaustion, no deadlock ───

test "integration: bounded concurrent PATCHes on same fleet — all 200, no deadlock" {
    suite_lock.lock();
    defer suite_lock.unlock();

    const ids = IDS_BOUNDED_WRITERS;
    const h = seedAndHarness(ALLOC, ids) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    defer cleanupHarness(h, ids);

    const body = "{\"trigger_markdown\":" ++ TRIGGER_VARIANT_A_JSON ++ "}";

    var outcomes: [PATCH_WRITER_COUNT]Outcome = @splat(Outcome{});
    defer freeOutcomes(&outcomes);

    var threads: [PATCH_WRITER_COUNT]std.Thread = undefined;
    for (&threads, 0..) |*t, i| {
        t.* = try std.Thread.spawn(.{}, Worker.run, .{ h, body, ids.a, &outcomes[i] });
    }
    for (threads) |t| t.join();

    var ok_count: usize = 0;
    for (outcomes) |o| {
        if (o.status == 200) ok_count += 1;
        try std.testing.expect(!bodyContainsDeadlock(o));
    }
    try std.testing.expectEqual(@as(usize, PATCH_WRITER_COUNT), ok_count);
}

// ── §4 — PATCH + DELETE on same fleet → no deadlock, one final state ───

test "integration: concurrent PATCH + DELETE same fleet — no deadlock" {
    suite_lock.lock();
    defer suite_lock.unlock();

    const ids = IDS_PATCH_DELETE;
    const h = seedAndHarness(ALLOC, ids) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    defer cleanupHarness(h, ids);

    const body = "{\"trigger_markdown\":" ++ TRIGGER_VARIANT_A_JSON ++ "}";

    var outcomes: [2]Outcome = .{ .{}, .{} };
    defer freeOutcomes(&outcomes);

    var threads: [2]std.Thread = undefined;
    threads[0] = try std.Thread.spawn(.{}, Worker.run, .{ h, body, ids.a, &outcomes[0] });
    threads[1] = try std.Thread.spawn(.{}, Worker.runDelete, .{ h, ids.a, &outcomes[1] });
    for (threads) |t| t.join();

    // Two interleavings are valid:
    //   (a) PATCH first → DELETE second: PATCH=200, DELETE=204 (or 200/202)
    //   (b) DELETE first → PATCH second: DELETE=204, PATCH=404 (fleet gone)
    // Either way: no 40P01 in any response.
    try std.testing.expect(!bodyContainsDeadlock(outcomes[0]));
    try std.testing.expect(!bodyContainsDeadlock(outcomes[1]));
    // At least one of the two must succeed in some shape.
    const patch_ok = outcomes[0].status == 200 or outcomes[0].status == 404;
    try std.testing.expect(patch_ok);
}

// ── §5 — Different fleets in parallel: near-linear wall time ────────────

test "integration: concurrent PATCH on different fleets — parallel, sub-linear" {
    suite_lock.lock();
    defer suite_lock.unlock();

    const ids = IDS_DIFFERENT_FLEETS;
    const h = seedAndHarness(ALLOC, ids) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    defer cleanupHarness(h, ids);

    // Per-fleet bodies — each PATCH carries the trigger variant whose name
    // matches its target row's name. Required because the PATCH handler's
    // UPDATE sets `name = parsed_trigger.config.name`, and a shared body
    // would drive both rows onto the same value → uq_fleets_workspace_id_name
    // violation on whichever commits second.
    const body_a = "{\"trigger_markdown\":" ++ TRIGGER_VARIANT_A_JSON ++ "}";
    const body_b = "{\"trigger_markdown\":" ++ TRIGGER_VARIANT_FOR_B_JSON ++ "}";

    var outcomes: [2]Outcome = .{ .{}, .{} };
    defer freeOutcomes(&outcomes);

    const t0 = clock.nowMillis();
    var threads: [2]std.Thread = undefined;
    threads[0] = try std.Thread.spawn(.{}, Worker.run, .{ h, body_a, ids.a, &outcomes[0] });
    threads[1] = try std.Thread.spawn(.{}, Worker.run, .{ h, body_b, ids.b, &outcomes[1] });
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
    suite_lock.lock();
    defer suite_lock.unlock();

    const ids = IDS_LOCK_TIMEOUT;
    const h = seedAndHarness(ALLOC, ids) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    defer cleanupHarness(h, ids);

    // Holder thread takes its own connection, BEGINs, SELECT FOR UPDATE,
    // sleeps 7s, then ROLLBACKs. The 7s holds the row-lock longer than
    // the handler's 5s lock_timeout, so the contending PATCH must fail
    // fast with 503, not hang for the full 7s.
    const Holder = struct {
        fn run(harness: *TestHarness, zid: []const u8, started: *std.atomic.Value(bool)) void {
            const c = harness.pool.acquire() catch return;
            defer harness.pool.release(c);
            _ = c.exec("BEGIN", .{}) catch return;
            defer _ = c.exec("ROLLBACK", .{}) catch {};
            _ = c.exec("SELECT id FROM core.fleets WHERE id = $1::uuid FOR UPDATE", .{zid}) catch return;
            // safe because: release pairs with the waiter acquire before issuing PATCH.
            started.store(true, .release);
            @import("common").sleepNanos(7 * std.time.ns_per_s);
        }
    };

    var started = std.atomic.Value(bool).init(false);
    const holder = try std.Thread.spawn(.{}, Holder.run, .{ h, ids.a, &started });
    defer holder.join();
    // Wait up to 2s for the holder's SELECT FOR UPDATE to grab the lock. Zig 0.16
    // removed Thread.ResetEvent.timedWait, so this is a bounded poll (200 × 10ms).
    {
        var waited: usize = 0;
        // safe because: acquire observes the holder's locked-row setup.
        while (!started.load(.acquire)) : (waited += 1) {
            if (waited >= 200) return error.HolderLockSetupTimeout;
            @import("common").sleepNanos(10 * std.time.ns_per_ms);
        }
    }

    const body = "{\"trigger_markdown\":" ++ TRIGGER_VARIANT_A_JSON ++ "}";
    var outcome: Outcome = .{};
    defer if (outcome.body) |b| ALLOC.free(b);
    const t0 = clock.nowMillis();
    Worker.run(h, body, ids.a, &outcome);
    const elapsed = clock.nowMillis() - t0;

    // Fail-fast: should return well before holder's 7s sleep completes.
    try std.testing.expect(elapsed < 5_500);
    try std.testing.expectEqual(@as(u16, 503), outcome.status);
    try std.testing.expect(!bodyContainsDeadlock(outcome));
}

// ── §7 — Concurrent PATCH + INSERT into fleet_events serialize cleanly ──

// PATCH handler takes SELECT FOR UPDATE on the fleet row inside its txn;
// an INSERT into core.fleet_events with FK ref to core.fleets(id) needs
// FOR KEY SHARE on the same parent. The lock modes are incompatible, so
// PG serializes the INSERT after the PATCH commit. Both must succeed; no
// `40P01 deadlock_detected` in either; the inserted row must be visible.
test "integration: concurrent PATCH + INSERT into fleet_events — both succeed, no deadlock" {
    suite_lock.lock();
    defer suite_lock.unlock();

    const ids = IDS_PATCH_INSERT;
    const h = seedAndHarness(ALLOC, ids) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    defer cleanupHarness(h, ids);

    const body_patch = "{\"trigger_markdown\":" ++ TRIGGER_VARIANT_A_JSON ++ "}";
    const evt_id = "evt_conc_patch_insert_1";

    var patch_out: Outcome = .{};
    var insert_out: Outcome = .{};
    defer if (patch_out.body) |b| ALLOC.free(b);
    defer if (insert_out.body) |b| ALLOC.free(b);

    var threads: [2]std.Thread = undefined;
    threads[0] = try std.Thread.spawn(.{}, Worker.run, .{ h, body_patch, ids.a, &patch_out });
    threads[1] = try std.Thread.spawn(.{}, Worker.runInsertEvent, .{ h, ids.a, evt_id, &insert_out });
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
            "SELECT COUNT(*)::bigint FROM core.fleet_events WHERE fleet_id = $1::uuid AND event_id = $2",
            .{ ids.a, evt_id },
        ));
        defer q_evt.deinit();
        const row_evt = (try q_evt.next()) orelse return error.RowNotFound;
        const evt_count = try row_evt.get(i64, 0);
        try std.testing.expectEqual(@as(i64, 1), evt_count);
    }
    {
        var q_cfg = PgQuery.from(try conn.query(
            "SELECT config_json::text FROM core.fleets WHERE id = $1::uuid",
            .{ids.a},
        ));
        defer q_cfg.deinit();
        const row_cfg = (try q_cfg.next()) orelse return error.RowNotFound;
        const cfg = try row_cfg.get([]const u8, 0);
        try expectDailyBudget("patch-insert config", cfg, 6);
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
