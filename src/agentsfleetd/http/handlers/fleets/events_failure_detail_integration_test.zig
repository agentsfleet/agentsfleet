// Durable proof for the M139 failure cause (§1 Dimensions 1.1 + 1.4): the
// `failure_detail` column added by migration 032 survives the report write and
// reaches the operator verbatim.
//
//   1.1  report write → store read: `markTerminal` persists the cause and
//        `listForFleet` returns the same bytes; a clean run stores NULL, so a
//        success can never carry a cause even at the row level.
//   1.4  runner report POST → GET /v1/workspaces/{ws}/fleets/{id}/events: the
//        envelope returns `failure_label` + `failure_detail` under those exact
//        names (Invariant 2 — the envelope is mirrored verbatim to the
//        dashboard, no shim, no rename).
//
// Both arms need the real column, so both need the migrated test DB. Requires
// TEST_DATABASE_URL — skipped gracefully otherwise.

const std = @import("std");
const clock = @import("common").clock;
const pg = @import("pg");

const scope_fixtures = @import("../../test_scope_tokens.zig");
const auth_mw = @import("../../../auth/middleware/mod.zig");
const serve_runner_lookup = @import("../../../cmd/serve_runner_lookup.zig");
const api_key = @import("../../../auth/api_key.zig");
const id_format = @import("../../../types/id_format.zig");
const event_rows = @import("../../../fleet/event_rows.zig");
const events_store = @import("../../../state/fleet_events_store.zig");
const protocol = @import("contract").protocol;
const execution_result = @import("contract").execution_result;
const ExecutionResult = execution_result.ExecutionResult;

const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

const ALLOC = std.testing.allocator;

// Tenant + workspace are the scope-token persona's own (the operator GET is
// authorized against them); the fleet id namespace is private to this suite so
// its rows never collide with a sibling's.
const TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
const FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dd101";
const RUNNER_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dd201";
const LEASE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dd301";
const AFFINITY_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dd401";
// The lease's token must equal the fleet's live fencing sequence, or the report
// is fenced UZ-RUN-005 as a superseded holder.
const FENCING_TOKEN: i64 = 1;
const OPERATOR_TOKEN = scope_fixtures.TENANT_ADMIN;
const RUNNER_TOKEN = auth_mw.runner_bearer.RUNNER_TOKEN_PREFIX ++ "e" ** 64;

const EVENT_FAILED = "evt-detail-failed";
const EVENT_CLEAN = "evt-detail-clean";
const EVENT_REPORTED = "evt-detail-reported";

// The cause lines under test. Distinct per arm so an assertion can never pass
// against the wrong row, and shaped like a real `startup_posture` cause.
const CAUSE_STORE = "startup check 'instructions' failed: no instructions configured";
const CAUSE_WIRE = "startup check 'model' failed: no model configured for this fleet";
const CLEAN_RESPONSE = "all good";

const LARGE_BALANCE_NANOS: i64 = 1_000_000_000_000;
const WALL_MS: u64 = 1_234;

// SAFETY: populated by configureRegistry before the middleware chain reads it.
var runner_lookup_ctx: serve_runner_lookup.Ctx = undefined;

fn configureRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    runner_lookup_ctx = .{ .pool = h.pool };
    reg.runner_bearer_mw = .{ .host = &runner_lookup_ctx, .lookup = serve_runner_lookup.lookup };
}

// One harness serves both arms: the runner_bearer chain for the report POST and
// the offline JWKS persona for the operator GET.
fn makeHarness() !*TestHarness {
    return TestHarness.start(ALLOC, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = scope_fixtures.JWKS,
        .issuer = scope_fixtures.ISSUER,
        .audience = scope_fixtures.AUDIENCE,
    });
}

// ── Seeds ───────────────────────────────────────────────────────────────────

fn seedBase(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO core.tenants (tenant_id, name, created_at, updated_at)
        \\VALUES ($1, 'EventsFailureDetailTest', 0, 0) ON CONFLICT DO NOTHING
    , .{TENANT_ID});
    _ = try conn.exec(
        \\INSERT INTO core.workspaces (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, 0) ON CONFLICT DO NOTHING
    , .{ WORKSPACE_ID, TENANT_ID });
    _ = try conn.exec(
        \\INSERT INTO core.fleets
        \\  (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, 'failure-detail-fleet', 'seed', '{}'::jsonb, 'active', 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ FLEET_ID, WORKSPACE_ID });
    _ = try conn.exec(
        \\INSERT INTO billing.tenant_billing (tenant_id, balance_nanos, grant_source, created_at, updated_at)
        \\VALUES ($1::uuid, $2, 'failure-detail-test', 0, 0)
        \\ON CONFLICT (tenant_id) DO UPDATE
        \\  SET balance_nanos = EXCLUDED.balance_nanos, balance_exhausted_at = NULL
    , .{ TENANT_ID, LARGE_BALANCE_NANOS });
}

fn seedRunner(conn: *pg.Conn) !void {
    const hash = api_key.sha256Hex(RUNNER_TOKEN);
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, 'failure-detail-host', $2, 'dev_none', 'active', '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ RUNNER_ID, hash[0..] });
}

// The fleet's authoritative fencing sequence. Without it the report's fence
// check sees the lease's token as stale and answers UZ-RUN-005.
fn seedAffinity(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_affinity
        \\  (id, fleet_id, last_runner_id, fencing_seq, leased_until,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens,
        \\   last_metered_at_ms, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, 0, 0, 0, 0, 0, 0)
        \\ON CONFLICT (fleet_id) DO UPDATE
        \\  SET last_runner_id = EXCLUDED.last_runner_id,
        \\      fencing_seq = EXCLUDED.fencing_seq,
        \\      leased_until = EXCLUDED.leased_until
    , .{ AFFINITY_ID, FLEET_ID, RUNNER_ID, FENCING_TOKEN, clock.nowMillis() + 600_000 });
}

fn seedActiveLease(conn: *pg.Conn, event_id: []const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_leases
        \\  (id, runner_id, fleet_id, workspace_id, tenant_id, event_id, actor,
        \\   event_type, request_json, event_created_at, posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens,
        \\   last_metered_at_ms, fencing_token, lease_expires_at, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, $6, 'steer:test',
        \\        'chat', '{"message":"hi"}', 0, 'platform', 'test-provider', 'test-model',
        \\        0, 0, 0, 0, $7, $8, 'active', 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ LEASE_ID, RUNNER_ID, FLEET_ID, WORKSPACE_ID, TENANT_ID, event_id, FENCING_TOKEN, clock.nowMillis() + 60_000 });
}

// A `received` row — the state the lease verb leaves behind and the only state
// `markTerminal`'s guarded UPDATE will transition.
fn seedReceivedEvent(conn: *pg.Conn, event_id: []const u8, ts: i64) !void {
    var uid_buf: [36]u8 = undefined;
    const uid = try id_format.formatUuidV7(&uid_buf);
    _ = try conn.exec(
        \\INSERT INTO core.fleet_events
        \\  (uid, fleet_id, event_id, workspace_id, actor, event_type, status,
        \\   request_json, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3, $4::uuid, 'steer:test', 'chat', $5,
        \\        '{"message":"hi"}'::jsonb, $6, $6)
        \\ON CONFLICT (fleet_id, event_id) DO NOTHING
    , .{ uid, FLEET_ID, event_id, WORKSPACE_ID, event_rows.STATUS_RECEIVED, ts });
}

fn execIgnore(conn: *pg.Conn, sql: []const u8, args: anytype) void {
    _ = conn.exec(sql, args) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

fn cleanup(conn: *pg.Conn) void {
    execIgnore(conn, "DELETE FROM fleet.metering_periods WHERE event_id = $1", .{EVENT_REPORTED});
    execIgnore(conn, "DELETE FROM core.fleet_execution_telemetry WHERE fleet_id = $1", .{FLEET_ID});
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE fleet_id = $1::uuid", .{FLEET_ID});
    execIgnore(conn, "DELETE FROM fleet.runner_affinity WHERE fleet_id = $1::uuid", .{FLEET_ID});
    execIgnore(conn, "DELETE FROM fleet.runners WHERE id = $1::uuid", .{RUNNER_ID});
    execIgnore(conn, "DELETE FROM core.fleet_events WHERE fleet_id = $1::uuid", .{FLEET_ID});
    execIgnore(conn, "DELETE FROM core.fleet_sessions WHERE fleet_id = $1::uuid", .{FLEET_ID});
    execIgnore(conn, "DELETE FROM core.fleets WHERE id = $1::uuid", .{FLEET_ID});
}

// ── Read helpers ────────────────────────────────────────────────────────────

fn listEvents(conn: *pg.Conn) ![]events_store.EventRow {
    return events_store.listForFleet(conn, ALLOC, WORKSPACE_ID, FLEET_ID, .{ .limit = 50 });
}

fn freeRows(rows: []events_store.EventRow) void {
    for (rows) |*r| r.deinit(ALLOC);
    ALLOC.free(rows);
}

fn rowFor(rows: []events_store.EventRow, event_id: []const u8) ?events_store.EventRow {
    for (rows) |r| {
        if (std.mem.eql(u8, r.event_id, event_id)) return r;
    }
    return null;
}

fn itemFor(items: std.json.Array, event_id: []const u8) ?std.json.ObjectMap {
    for (items.items) |item| {
        const id = item.object.get("event_id") orelse continue;
        if (id == .string and std.mem.eql(u8, id.string, event_id)) return item.object;
    }
    return null;
}

// ── Tests ───────────────────────────────────────────────────────────────────

test "integration: the terminal write roundtrips failure_detail and leaves a clean run's cause NULL" {
    const h = makeHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanup(conn);
    defer cleanup(conn);

    try seedBase(conn);
    const now_ms = clock.nowMillis();
    try seedReceivedEvent(conn, EVENT_FAILED, now_ms);
    try seedReceivedEvent(conn, EVENT_CLEAN, now_ms + 1);

    // The report write the runner's failure produces — the cause rides the
    // failed variant, so `markTerminal` reads it off the same value it reads
    // the verdict off.
    const failed = ExecutionResult{
        .outcome = .{ .failed = .{ .class = .startup_posture, .detail = CAUSE_STORE } },
    };
    event_rows.markTerminal(h.pool, FLEET_ID, EVENT_FAILED, failed, WALL_MS);
    event_rows.markTerminal(h.pool, FLEET_ID, EVENT_CLEAN, ExecutionResult.completed(CLEAN_RESPONSE), WALL_MS);

    const rows = try listEvents(conn);
    defer freeRows(rows);

    const bad = rowFor(rows, EVENT_FAILED) orelse return error.FailedRowMissing;
    try std.testing.expectEqualStrings(event_rows.STATUS_FLEET_ERROR, bad.status);
    try std.testing.expectEqualStrings("startup_posture", bad.failure_label orelse return error.LabelMissing);
    // The whole point: the same bytes the classification site named come back
    // off the migrated column, not a class tag standing in for a cause.
    try std.testing.expectEqualStrings(CAUSE_STORE, bad.failure_detail orelse return error.DetailMissing);

    // A clean run has no cause to carry, and the row proves it — the union's
    // invariant holds all the way down to the column (negative arm).
    const ok = rowFor(rows, EVENT_CLEAN) orelse return error.CleanRowMissing;
    try std.testing.expectEqualStrings(event_rows.STATUS_PROCESSED, ok.status);
    try std.testing.expect(ok.failure_label == null);
    try std.testing.expect(ok.failure_detail == null);
}

test "integration: a failed runner report persists the cause and the events envelope returns it verbatim" {
    const h = makeHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    cleanup(conn);
    defer cleanup(conn);

    try seedBase(conn);
    try seedRunner(conn);
    try seedReceivedEvent(conn, EVENT_REPORTED, clock.nowMillis());
    try seedAffinity(conn);
    try seedActiveLease(conn, EVENT_REPORTED);

    // The report as the runner actually sends it: the flat, defaulted wire
    // struct, serialized from the type itself so no key is hand-spelled here.
    const report = protocol.ReportRequest{
        .lease_id = LEASE_ID,
        .event_id = EVENT_REPORTED,
        .fencing_token = @intCast(FENCING_TOKEN),
        .outcome = .fleet_error,
        .failure_reason = .startup_posture,
        .failure_detail = CAUSE_WIRE,
        .response_text = "",
        .tokens = 0,
        .telemetry = .{ .time_to_first_token_ms = 0, .wall_ms = WALL_MS },
        .checkpoint = .{ .last_event_id = EVENT_REPORTED, .last_response = "" },
    };
    const body = try std.json.Stringify.valueAlloc(ALLOC, report, .{});
    defer ALLOC.free(body);
    const rep = try (try (try h.post(protocol.PATH_RUNNER_REPORTS).bearer(RUNNER_TOKEN)).json(body)).send();
    defer rep.deinit();
    try rep.expectStatus(.ok);

    // The operator now reads the same event off the console's endpoint.
    const url = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/fleets/{s}/events", .{ WORKSPACE_ID, FLEET_ID });
    defer ALLOC.free(url);
    const listed = try (try h.get(url).bearer(OPERATOR_TOKEN)).send();
    defer listed.deinit();
    try listed.expectStatus(.ok);

    const parsed = try std.json.parseFromSlice(std.json.Value, ALLOC, listed.body, .{});
    defer parsed.deinit();
    const items = (parsed.value.object.get("items") orelse return error.ItemsMissing).array;
    const item = itemFor(items, EVENT_REPORTED) orelse return error.ReportedEventMissing;

    // Field names asserted literally: the envelope is mirrored verbatim to the
    // dashboard, so a rename here is a break the dashboard cannot absorb.
    try std.testing.expectEqualStrings(
        event_rows.STATUS_FLEET_ERROR,
        (item.get("status") orelse return error.StatusMissing).string,
    );
    try std.testing.expectEqualStrings(
        "startup_posture",
        (item.get("failure_label") orelse return error.LabelFieldMissing).string,
    );
    try std.testing.expectEqualStrings(
        CAUSE_WIRE,
        (item.get("failure_detail") orelse return error.DetailFieldMissing).string,
    );
}
