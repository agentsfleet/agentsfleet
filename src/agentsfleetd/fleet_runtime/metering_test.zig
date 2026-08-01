// Tests for src/agentsfleetd/fleet_runtime/metering.zig — two-debit credit-pool path.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;

const metering = @import("metering.zig");
const tenant_billing = @import("../state/tenant_billing.zig");
const base = @import("../db/test_fixtures.zig");

const ALLOC = std.testing.allocator;

// Per-suite tenant (fa05 block, matching the aa05 workspace segment): keeps
// this suite's grants/resets off every other suite's balance assertions.
const TENANT_ID = "0195b4ba-8d3a-7f13-8abc-fa0500000000";

const WS_GATE_PASS = "0195b4ba-8d3a-7f13-8abc-aa0500000001";
const WS_GATE_BLOCK = "0195b4ba-8d3a-7f13-8abc-aa0500000002";
const WS_RECEIVE_DEBIT = "0195b4ba-8d3a-7f13-8abc-aa0500000003";
const WS_GATE_COVERED = "0195b4ba-8d3a-7f13-8abc-aa0500000004";

fn seed(conn: *pg.Conn, workspace_id: []const u8) !void {
    try base.seedTenantById(conn, TENANT_ID, "metering-suite");
    try base.seedWorkspaceWithTenant(conn, workspace_id, TENANT_ID);
}

fn teardown(conn: *pg.Conn, workspace_id: []const u8) void {
    base.teardownWorkspace(conn, workspace_id);
    base.teardownTenantById(conn, TENANT_ID);
}

fn makeCtx(workspace_id: []const u8, event_id: []const u8) metering.PreflightContext {
    return .{
        .workspace_id = workspace_id,
        .fleet_id = "fleet-test",
        .event_id = event_id,
        .posture = .self_managed,
        .provider = "self-managed-test",
        .model = "any-model-self-managed-doesnt-need-cache",
    };
}

test "DebitOutcome variants compile and pattern-match" {
    const r1: metering.DebitOutcome = .{ .deducted = 7 };
    const r2: metering.DebitOutcome = .{ .exhausted = {} };
    const r3: metering.DebitOutcome = .{ .missing_tenant_billing = {} };
    const r4: metering.DebitOutcome = .{ .db_error = {} };

    try std.testing.expectEqual(@as(i64, 7), switch (r1) {
        .deducted => |c| c,
        else => @as(i64, -1),
    });
    switch (r2) {
        .exhausted => {},
        else => return error.TestExpectedEqual,
    }
    switch (r3) {
        .missing_tenant_billing => {},
        else => return error.TestExpectedEqual,
    }
    switch (r4) {
        .db_error => {},
        else => return error.TestExpectedEqual,
    }
}

test "balanceCoversEstimate: returns true under non-stop policies regardless of balance" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try seed(db_ctx.conn, WS_GATE_PASS);
    defer teardown(db_ctx.conn, WS_GATE_PASS);

    // Provision at 0¢ — balance is empty, but non-stop policies must let
    // the event through.
    try tenant_billing.provision(db_ctx.conn, TENANT_ID, 0, "test_continue");

    try std.testing.expect(metering.balanceCoversEstimate(
        db_ctx.pool,
        ALLOC,
        TENANT_ID,
        .self_managed,
        "self-managed-test",
        "any-model",
        .@"continue",
    ));
}

test "balanceCoversEstimate: blocks when stop policy AND balance below est_total" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try seed(db_ctx.conn, WS_GATE_BLOCK);
    defer teardown(db_ctx.conn, WS_GATE_BLOCK);
    // Platform posture so est_total (the token floor) is non-zero; self_managed
    // carries no issue-time charge under the run-fee model.
    try base.seedPlatformProvider(ALLOC, db_ctx.conn, WS_GATE_BLOCK);
    defer base.teardownPlatformProvider(db_ctx.conn, WS_GATE_BLOCK);

    // platform: receive = EVENT_NANOS (0), stage = token-floor cost (run fee is
    // 0 at issue). Balance 0 < est_total → blocked. seedPlatformProvider just
    // granted the starter balance onto this suite's tenant and provision is
    // idempotent — reset so the 0 actually lands.
    base.resetBillingFor(db_ctx.conn, TENANT_ID);
    try tenant_billing.provision(db_ctx.conn, TENANT_ID, 0, "test_block");

    // Free-trial: stage charge short-circuits to 0 until FREE_TRIAL_END_MS, so
    // est_total = 0 and `balance < est_total` is mathematically unreachable.
    // Skip until post-trial — the gate still holds, this assertion just can't
    // be exercised through the public charge path while the gate is closed.
    const trial_active = blk: {
        const b = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, TENANT_ID)).?;
        defer ALLOC.free(@constCast(b.grant_source));
        break :blk b.free_trial_active;
    };
    if (trial_active) return error.SkipZigTest;

    try std.testing.expect(!metering.balanceCoversEstimate(
        db_ctx.pool,
        ALLOC,
        TENANT_ID,
        .platform,
        "anthropic",
        "claude-sonnet-4-6",
        .stop,
    ));
}

test "balanceCoversEstimate: passes when stop policy AND balance covers est_total" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try seed(db_ctx.conn, WS_GATE_COVERED);
    defer teardown(db_ctx.conn, WS_GATE_COVERED);

    try tenant_billing.insertStarterGrant(db_ctx.conn, TENANT_ID);

    // STARTER_CREDIT_NANOS trivially covers a self_managed event — receive is
    // EVENT_NANOS (0) and the issue-time run fee is 0, so est_total is 0.
    try std.testing.expect(metering.balanceCoversEstimate(
        db_ctx.pool,
        ALLOC,
        TENANT_ID,
        .self_managed,
        "self-managed-test",
        "any-model",
        .stop,
    ));
}

test "debitReceive self-managed: EVENT_NANOS=0 charge writes telemetry row, balance unchanged" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try seed(db_ctx.conn, WS_RECEIVE_DEBIT);
    defer teardown(db_ctx.conn, WS_RECEIVE_DEBIT);
    defer _ = db_ctx.conn.exec("DELETE FROM core.fleet_execution_telemetry WHERE workspace_id = $1", .{WS_RECEIVE_DEBIT}) catch {};

    base.resetBillingFor(db_ctx.conn, TENANT_ID);
    try tenant_billing.insertStarterGrant(db_ctx.conn, TENANT_ID);

    const event_id = "0195b4ba-8d3a-7f13-8abc-aa1900000a01";
    const result = metering.debitReceive(
        db_ctx.pool,
        ALLOC,
        TENANT_ID,
        makeCtx(WS_RECEIVE_DEBIT, event_id),
        EVENT_CREATED_AT,
        .stop,
    );
    switch (result) {
        .deducted => |c| try std.testing.expectEqual(tenant_billing.EVENT_NANOS, c),
        else => return error.TestExpectedEqual,
    }

    // Balance unchanged — receive charges EVENT_NANOS (zero) under both postures.
    const row = (try tenant_billing.getBilling(db_ctx.conn, ALLOC, TENANT_ID)).?;
    defer ALLOC.free(@constCast(row.grant_source));
    try std.testing.expectEqual(tenant_billing.STARTER_CREDIT_NANOS, row.balance_nanos);

    // Telemetry row must exist with charge_type='receive'.
    var q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT charge_type, posture, credit_deducted_nanos
        \\FROM core.fleet_execution_telemetry WHERE event_id = $1
    , .{event_id}));
    defer q.deinit();
    const r = (try q.next()) orelse return error.RowNotFound;
    try std.testing.expectEqualStrings("receive", try r.get([]const u8, 0));
    try std.testing.expectEqualStrings("self_managed", try r.get([]const u8, 1));
    try std.testing.expectEqual(tenant_billing.EVENT_NANOS, try r.get(i64, 2));
}

test "telemetry insert is idempotent: same event_id+charge_type replayed inserts 1 row" {
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    const ws = "0195b4ba-8d3a-7f13-8abc-aa0500000008";
    try seed(db_ctx.conn, ws);
    defer teardown(db_ctx.conn, ws);
    defer _ = db_ctx.conn.exec("DELETE FROM core.fleet_execution_telemetry WHERE workspace_id = $1", .{ws}) catch {};

    base.resetBillingFor(db_ctx.conn, TENANT_ID);
    try tenant_billing.insertStarterGrant(db_ctx.conn, TENANT_ID);

    const event_id = "0195b4ba-8d3a-7f13-8abc-aa1900000a06";
    const ctx = makeCtx(ws, event_id);

    _ = metering.debitReceive(db_ctx.pool, ALLOC, TENANT_ID, ctx, EVENT_CREATED_AT, .stop);
    // Replay: the second INSERT must hit ON CONFLICT DO NOTHING.
    _ = metering.debitReceive(db_ctx.pool, ALLOC, TENANT_ID, ctx, EVENT_CREATED_AT, .stop);

    var q = PgQuery.from(try db_ctx.conn.query(
        \\SELECT COUNT(*)::BIGINT FROM core.fleet_execution_telemetry
        \\WHERE event_id = $1 AND charge_type = 'receive'
    , .{event_id}));
    defer q.deinit();
    const r = (try q.next()).?;
    try std.testing.expectEqual(@as(i64, 1), try r.get(i64, 0));
}

// ── The `fleet.delivery` span ────────────────────────────────────────────
//
// The span is a CUSTOM control-plane observation, not a claimed runner trace:
// the runner produces no span and propagates no trace context. These tests pin
// two things the schema depends on — the attribute set is exactly the standard
// GenAI keys plus namespaced product correlations, and prompt/response content
// never reaches it. No database: the emit path is observability only.

const otel_traces = @import("../observability/otel_traces.zig");
const otlp_config = @import("../observability/otlp/config.zig");
const semconv = @import("../observability/semconv.zig");
const tenant_provider = @import("../state/tenant_provider.zig");

/// The event envelope's creation instant. Fixed rather than `nowMillis()`:
/// every ledger row for one event must carry the SAME value (schema/710),
/// which a per-call clock read cannot guarantee.
const EVENT_CREATED_AT: i64 = 1_760_000_000_000;

const SPAN_TEST_CFG: otlp_config.GrafanaOtlpConfig = .{
    .endpoint = "http://127.0.0.1:0",
    .instance_id = "i",
    .api_key = "k",
};

const SPAN_TENANT = "0195b4ba-8d3a-7f13-8abc-fa0500000099";
const SPAN_EPOCH_MS: i64 = 1_700_000_000_000;
/// A believable run length for the span window; any positive value works.
const SPAN_WALL_MS: u64 = 1_000;

fn deliveryContext(provider: []const u8) metering.PreflightContext {
    return .{
        .workspace_id = "0195b4ba-8d3a-7f13-8abc-aa0500000099",
        .fleet_id = "0195b4ba-8d3a-7f13-8abc-bb0500000099",
        .event_id = "0195b4ba-8d3a-7f13-8abc-cc0500000099",
        .posture = tenant_provider.Mode.platform,
        .provider = provider,
        .model = "claude-opus-4-8",
    };
}

test "test_delivery_span_uses_semantic_attributes_without_runner_claim" {
    otel_traces.testClear();
    defer otel_traces.testClear();
    otel_traces.testSetInstalled(SPAN_TEST_CFG);

    metering.emitDeliverySpan(SPAN_TENANT, deliveryContext("anthropic"), 80, 30, 5_000, SPAN_EPOCH_MS);
    try std.testing.expectEqual(@as(usize, 1), otel_traces.testPendingCount());

    const body = (try otel_traces.testCollect(ALLOC, SPAN_TEST_CFG)) orelse return error.NoBody;
    defer ALLOC.free(body);

    var doc = try std.json.parseFromSlice(std.json.Value, ALLOC, body, .{});
    defer doc.deinit();

    // A custom control-plane observation, not a client span: INTERNAL kind,
    // product span name. CLIENT would assert this process called the provider.
    try std.testing.expect(std.mem.indexOf(u8, body, "\"name\":\"" ++ semconv.SPAN_FLEET_DELIVERY ++ "\"") != null);
    var kind_buf: [32]u8 = undefined;
    const internal_kind = try std.fmt.bufPrint(&kind_buf, "\"kind\":{d}", .{@intFromEnum(otel_traces.SpanKind.internal)});
    try std.testing.expect(std.mem.indexOf(u8, body, internal_kind) != null);

    // Standard where the fact is standard.
    try std.testing.expect(std.mem.indexOf(u8, body, semconv.ATTR_OPERATION_NAME) != null);
    try std.testing.expect(std.mem.indexOf(u8, body, "\"stringValue\":\"" ++ semconv.OPERATION_INVOKE_AGENT ++ "\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, body, semconv.ATTR_AGENT_ID) != null);
    try std.testing.expect(std.mem.indexOf(u8, body, semconv.ATTR_PROVIDER_NAME) != null);
    try std.testing.expect(std.mem.indexOf(u8, body, semconv.ATTR_REQUEST_MODEL) != null);

    // Usage counts land in the TYPED `intValue` slot, never `stringValue`, so a
    // backend aggregates them instead of treating tokens as an opaque label.
    // (OTLP-JSON still spells a 64-bit int as a quoted string — that is the
    // wire encoding for the int type, not a string-typed attribute.)
    try std.testing.expect(std.mem.indexOf(u8, body, "\"intValue\":\"80\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, body, "\"intValue\":\"30\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, body, "\"stringValue\":\"80\"") == null);

    // Correlation identity is allowed on a SPAN (bounded per-event record) even
    // though it is forbidden on a METRIC (unbounded series).
    for ([_][]const u8{ semconv.ATTR_WORKSPACE_ID, semconv.ATTR_TENANT_ID, semconv.ATTR_EVENT_ID }) |key| {
        try std.testing.expect(std.mem.indexOf(u8, body, key) != null);
    }
}

test "the delivery span carries no prompt, response, or credential material" {
    otel_traces.testClear();
    defer otel_traces.testClear();
    otel_traces.testSetInstalled(SPAN_TEST_CFG);

    metering.emitDeliverySpan(SPAN_TENANT, deliveryContext("anthropic"), 1, 1, SPAN_WALL_MS, SPAN_EPOCH_MS);
    const body = (try otel_traces.testCollect(ALLOC, SPAN_TEST_CFG)) orelse return error.NoBody;
    defer ALLOC.free(body);

    // The emit signature cannot even receive content — this pins that a future
    // edit does not quietly widen it. Names come from the pinned GenAI content
    // conventions this process deliberately does not implement.
    for ([_][]const u8{
        "gen_ai.prompt",
        "gen_ai.completion",
        "gen_ai.input.messages",
        "gen_ai.output.messages",
        "response_text",
        "api_key",
        "authorization",
    }) |forbidden| {
        if (std.mem.indexOf(u8, body, forbidden) != null) {
            std.debug.print("FAIL: delivery span carries `{s}`\n", .{forbidden});
            return error.DeliverySpanCarriesContent;
        }
    }
}

test "an unknown provider omits gen_ai.provider.name rather than inventing one" {
    otel_traces.testClear();
    defer otel_traces.testClear();
    otel_traces.testSetInstalled(SPAN_TEST_CFG);

    // "megacorp-ai" maps to no well-known name. Exporting it under the standard
    // key would publish a private spelling as though upstream defined it.
    metering.emitDeliverySpan(SPAN_TENANT, deliveryContext("megacorp-ai"), 1, 1, SPAN_WALL_MS, SPAN_EPOCH_MS);
    const body = (try otel_traces.testCollect(ALLOC, SPAN_TEST_CFG)) orelse return error.NoBody;
    defer ALLOC.free(body);

    try std.testing.expect(std.mem.indexOf(u8, body, semconv.ATTR_PROVIDER_NAME) == null);
    try std.testing.expect(std.mem.indexOf(u8, body, "megacorp-ai") == null);
    // The measurement itself survives the omission.
    try std.testing.expect(std.mem.indexOf(u8, body, semconv.ATTR_REQUEST_MODEL) != null);
}

test "a non-positive epoch emits no span rather than a corrupt timeline" {
    otel_traces.testClear();
    defer otel_traces.testClear();
    otel_traces.testSetInstalled(SPAN_TEST_CFG);

    metering.emitDeliverySpan(SPAN_TENANT, deliveryContext("anthropic"), 1, 1, SPAN_WALL_MS, 0);
    metering.emitDeliverySpan(SPAN_TENANT, deliveryContext("anthropic"), 1, 1, SPAN_WALL_MS, -1);
    try std.testing.expectEqual(@as(usize, 0), otel_traces.testPendingCount());
}
