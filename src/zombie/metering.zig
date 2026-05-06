//! Two-debit metering for the credit-pool billing model.
//!
//! Each event drains credits twice: a small `receive` charge after the
//! balance gate passes (work-already-done semantics), and a `stage` charge
//! before the executor starts. Each debit pairs with a telemetry-row INSERT
//! inside one transaction. After execution, the stage telemetry row is
//! updated with the executor's reported token counts and wall_ms — the
//! debit cents stay at the conservative pre-execution estimate; later
//! reconciliation can adjust it if the product needs that.
//!
//! Replay safety. The telemetry table has UNIQUE (event_id, charge_type) +
//! ON CONFLICT DO NOTHING; same event id replayed produces zero extra rows.
//! Debit idempotency is best-effort before GA; without
//! an audit table, a worker crash between debit and INSERT can produce a
//! debited-but-unrecorded charge. Acceptable until Stripe wires in.
//!
//! All DB failures are non-fatal: callers receive `.db_error` and the
//! event loop XACKs the event so it isn't redelivered into the same fault.
//! Cents already drained on a partial transaction (receive succeeds, stage
//! fails) are NOT refunded — receive is "we did the work to evaluate it"
//! and stays charged on a stage-side exhaust.

const std = @import("std");
const logging = @import("log");
const pg = @import("pg");
const Allocator = std.mem.Allocator;

const tenant_billing = @import("../state/tenant_billing.zig");
const tenant_provider = @import("../state/tenant_provider.zig");
const zombie_telemetry_store = @import("../state/zombie_telemetry_store.zig");
const otel_traces = @import("../observability/otel_traces.zig");
const trace = @import("../observability/trace.zig");
const balance_policy = @import("../config/balance_policy.zig");

const log = logging.scoped(.zombie_metering);

/// Per-event context shared by the gate, both debits, and post-execution
/// telemetry. Posture and model come from the resolver; everything else
/// flows through from the worker.
pub const PreflightContext = struct {
    workspace_id: []const u8,
    zombie_id: []const u8,
    event_id: []const u8,
    posture: tenant_provider.Mode,
    model: []const u8,
};

pub const DebitOutcome = union(enum) {
    /// Debit + telemetry both committed. Cents drained on this charge.
    deducted: i64,
    /// Balance < cents. Tenant balance unchanged. Caller marks gate_blocked.
    /// On the *first* exhaust the row's `balance_exhausted_at` is stamped
    /// inside the same transaction (atomic with the failed debit attempt).
    exhausted: void,
    /// Tenant has no billing row — bootstrap invariant violated. Logged at
    /// `err`; caller should sleep + return without XACK so the operator can
    /// fix the bootstrap and the event redelivers cleanly.
    missing_tenant_billing: void,
    /// Non-fatal DB failure. Caller XACKs to avoid retrying into the fault.
    db_error: void,
};

/// Pre-claim balance gate. Reads tenant balance, compares against the
/// estimated total cost (receive + stage at floor tokens). Returns true
/// iff the tenant has enough credit to cover the conservative estimate.
///
/// Policy `continue`/`warn` short-circuits to true: those modes deliberately
/// allow the event through and emit warning telemetry instead of blocking.
/// Default policy is `stop`; non-stop modes are kept for the existing policy
/// hooks.
///
/// Any DB failure returns true (fail-open) so the gate never turns into an
/// availability incident.
pub fn balanceCoversEstimate(
    pool: *pg.Pool,
    alloc: Allocator,
    tenant_id: []const u8,
    posture: tenant_provider.Mode,
    model: []const u8,
    policy: balance_policy.Policy,
) bool {
    if (policy != .stop) return true;

    const conn = pool.acquire() catch |err| {
        log.warn("gate_acquire_fail", .{ .tenant_id = tenant_id, .err = @errorName(err) });
        return true;
    };
    defer pool.release(conn);

    const billing = (tenant_billing.getBilling(conn, alloc, tenant_id) catch |err| {
        log.warn("gate_billing_load_fail", .{ .tenant_id = tenant_id, .err = @errorName(err) });
        return true;
    }) orelse return true;
    defer alloc.free(@constCast(billing.plan_tier));
    defer alloc.free(@constCast(billing.plan_sku));
    defer alloc.free(@constCast(billing.grant_source));

    const receive = tenant_billing.computeReceiveCharge(posture);
    const stage = tenant_billing.computeStageCharge(
        posture,
        model,
        tenant_billing.ESTIMATE_FLOOR_INPUT_TOKENS,
        tenant_billing.ESTIMATE_FLOOR_OUTPUT_TOKENS,
    );
    return billing.balance_cents >= (receive + stage);
}

/// Charge `computeReceiveCharge(ctx.posture)` and INSERT a `receive`
/// telemetry row. Both ops in a single transaction; rollback on either
/// failure leaves the balance untouched and the row absent.
pub fn debitReceive(
    pool: *pg.Pool,
    alloc: Allocator,
    tenant_id: []const u8,
    ctx: PreflightContext,
    policy: balance_policy.Policy,
) DebitOutcome {
    const cents = tenant_billing.computeReceiveCharge(ctx.posture);
    return debitAndInsert(pool, alloc, tenant_id, ctx, .receive, cents, policy);
}

/// Charge `computeStageCharge(posture, model, FLOOR_INPUT, FLOOR_OUTPUT)`
/// and INSERT a `stage` telemetry row with NULL token counts and wall_ms.
/// The conservative estimate is the charge — `recordStageActuals` later
/// only updates the token/wall fields, never the cents.
pub fn debitStage(
    pool: *pg.Pool,
    alloc: Allocator,
    tenant_id: []const u8,
    ctx: PreflightContext,
    policy: balance_policy.Policy,
) DebitOutcome {
    const cents = tenant_billing.computeStageCharge(
        ctx.posture,
        ctx.model,
        tenant_billing.ESTIMATE_FLOOR_INPUT_TOKENS,
        tenant_billing.ESTIMATE_FLOOR_OUTPUT_TOKENS,
    );
    return debitAndInsert(pool, alloc, tenant_id, ctx, .stage, cents, policy);
}

/// Post-execution: UPDATE the stage telemetry row's token counts and
/// wall_ms with the executor's actual reported values. Build the OTel
/// span here from the same (zombie_id, workspace_id, tenant_id, event_id,
/// token_count) tuple recorded into the telemetry rows. Fire-and-forget;
/// any DB failure logs and continues so the event loop reaches XACK.
pub fn recordStageActuals(
    pool: *pg.Pool,
    alloc: Allocator,
    tenant_id: []const u8,
    ctx: PreflightContext,
    token_count_input: u64,
    token_count_output: u64,
    wall_ms: u64,
    epoch_wall_time_ms: i64,
) void {
    _ = alloc;
    if (epoch_wall_time_ms <= 0) {
        log.warn("skip_actuals", .{ .reason = "non_positive_epoch", .zombie_id = ctx.zombie_id });
        return;
    }
    const conn = pool.acquire() catch |err| {
        log.warn("actuals_acquire_fail", .{ .zombie_id = ctx.zombie_id, .err = @errorName(err) });
        return;
    };
    defer pool.release(conn);

    const in_capped: i64 = @intCast(@min(token_count_input, @as(u64, @intCast(std.math.maxInt(i64)))));
    const out_capped: i64 = @intCast(@min(token_count_output, @as(u64, @intCast(std.math.maxInt(i64)))));
    const wall_capped: i64 = @intCast(@min(wall_ms, @as(u64, @intCast(std.math.maxInt(i64)))));
    zombie_telemetry_store.updateStageTokens(conn, ctx.event_id, in_capped, out_capped, wall_capped) catch |err| {
        log.warn("stage_update_fail", .{ .zombie_id = ctx.zombie_id, .event_id = ctx.event_id, .err = @errorName(err) });
    };

    const total_tokens: u64 = token_count_input + token_count_output;
    const start_ns: u64 = @as(u64, @intCast(epoch_wall_time_ms)) * 1_000_000;
    const wall_seconds_capped: u64 = @min(wall_ms / 1_000, 604_800);
    const end_ns: u64 = start_ns + wall_seconds_capped * 1_000_000_000;
    const tctx = trace.TraceContext.generate();
    var span = otel_traces.buildSpan(tctx, "zombie.delivery", start_ns, end_ns);
    _ = otel_traces.addAttr(&span, "zombie_id", ctx.zombie_id);
    _ = otel_traces.addAttr(&span, "workspace_id", ctx.workspace_id);
    _ = otel_traces.addAttr(&span, "tenant_id", tenant_id);
    _ = otel_traces.addAttr(&span, "event_id", ctx.event_id);
    _ = otel_traces.addAttr(&span, "posture", ctx.posture.label());
    _ = otel_traces.addAttr(&span, "model", ctx.model);
    var cnt_buf: [24]u8 = undefined;
    const cnt_str = std.fmt.bufPrint(&cnt_buf, "{d}", .{total_tokens}) catch "0";
    _ = otel_traces.addAttr(&span, "token_count", cnt_str);
    otel_traces.enqueueSpan(span);
}

// ── Internal helpers ─────────────────────────────────────────────────────

fn debitAndInsert(
    pool: *pg.Pool,
    alloc: Allocator,
    tenant_id: []const u8,
    ctx: PreflightContext,
    charge_type: zombie_telemetry_store.ChargeType,
    cents: i64,
    policy: balance_policy.Policy,
) DebitOutcome {
    const conn = pool.acquire() catch |err| {
        log.warn("acquire_fail", .{ .zombie_id = ctx.zombie_id, .err = @errorName(err) });
        return .{ .db_error = {} };
    };
    defer pool.release(conn);

    _ = conn.exec("BEGIN", .{}) catch |err| {
        log.warn("begin_fail", .{ .zombie_id = ctx.zombie_id, .err = @errorName(err) });
        return .{ .db_error = {} };
    };
    var tx_open = true;
    defer if (tx_open) {
        conn.rollback() catch {};
    };

    if (cents > 0) {
        _ = tenant_billing.debit(conn, tenant_id, cents) catch |err| switch (err) {
            error.CreditExhausted => {
                _ = tenant_billing.markExhausted(conn, tenant_id) catch |mark_err| {
                    log.warn("mark_exhausted_fail", .{ .zombie_id = ctx.zombie_id, .tenant_id = tenant_id, .err = @errorName(mark_err) });
                };
                _ = conn.exec("COMMIT", .{}) catch {};
                tx_open = false;
                onExhaustedDebit(ctx.zombie_id, tenant_id, charge_type, cents, policy);
                return .{ .exhausted = {} };
            },
            error.TenantBillingMissing => {
                conn.rollback() catch {};
                tx_open = false;
                log.err("missing_tenant_billing", .{
                    .zombie_id = ctx.zombie_id,
                    .tenant_id = tenant_id,
                    .workspace_id = ctx.workspace_id,
                    .msg = "starter grant was never inserted for this tenant",
                });
                return .{ .missing_tenant_billing = {} };
            },
            else => {
                conn.rollback() catch {};
                tx_open = false;
                log.warn("debit_fail", .{ .zombie_id = ctx.zombie_id, .tenant_id = tenant_id, .err = @errorName(err) });
                return .{ .db_error = {} };
            },
        };
    }

    zombie_telemetry_store.insertTelemetry(conn, alloc, .{
        .tenant_id = tenant_id,
        .workspace_id = ctx.workspace_id,
        .zombie_id = ctx.zombie_id,
        .event_id = ctx.event_id,
        .charge_type = charge_type,
        .posture = ctx.posture,
        .model = ctx.model,
        .credit_deducted_cents = cents,
        .token_count_input = null,
        .token_count_output = null,
        .wall_ms = null,
        .recorded_at = std.time.milliTimestamp(),
    }) catch |err| {
        conn.rollback() catch {};
        tx_open = false;
        log.warn("telemetry_insert_fail", .{ .zombie_id = ctx.zombie_id, .event_id = ctx.event_id, .charge_type = charge_type.label(), .err = @errorName(err) });
        return .{ .db_error = {} };
    };

    _ = conn.exec("COMMIT", .{}) catch |err| {
        log.warn("commit_fail", .{ .zombie_id = ctx.zombie_id, .err = @errorName(err) });
        return .{ .db_error = {} };
    };
    tx_open = false;

    log.debug("debit", .{ .charge_type = charge_type.label(), .tenant_id = tenant_id, .event_id = ctx.event_id, .cents = cents });
    return .{ .deducted = cents };
}

fn onExhaustedDebit(
    zombie_id: []const u8,
    tenant_id: []const u8,
    charge_type: zombie_telemetry_store.ChargeType,
    cents: i64,
    policy: balance_policy.Policy,
) void {
    log.info("exhausted", .{
        .zombie_id = zombie_id,
        .tenant_id = tenant_id,
        .charge_type = charge_type.label(),
        .cents_attempted = cents,
        .policy = policy.label(),
    });
}

test {
    _ = @import("metering_test.zig");
}
