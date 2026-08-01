//! Issue-time metering for the credit-pool billing model.
//!
//! A `receive` charge drains credits once, after the balance gate passes
//! (work-already-done semantics), paired with a telemetry-row INSERT inside one
//! transaction. The `stage` cost is no longer a one-shot estimate taken here:
//! it is metered incrementally as the run proceeds — the run fee + per-token
//! delta is charged on every `/renew` and the final slice is settled at report
//! (see `fleet/renewal.zig` and `fleet/service_report.zig`). So this module
//! gates + charges receive at issue; the per-event `stage` telemetry row is
//! created and accumulated by the renewal/settle CTE, not here.
//!
//! Replay safety. The telemetry table has UNIQUE (event_id, charge_type) +
//! ON CONFLICT DO NOTHING; same event id replayed produces zero extra rows.
//! Debit idempotency is best-effort before GA; without
//! an audit table, a worker crash between debit and INSERT can produce a
//! debited-but-unrecorded charge. Acceptable until Stripe wires in.
//!
//! All DB failures are non-fatal: callers receive `.db_error` and the
//! event loop XACKs the event so it isn't redelivered into the same fault.

const std = @import("std");
const clock = @import("common").clock;
const logging = @import("log");
const ec = @import("../errors/error_registry.zig");
const pg = @import("pg");
const Allocator = std.mem.Allocator;

const tenant_billing = @import("../state/tenant_billing.zig");
const billing_rates = @import("../state/tenant_billing_rates.zig");
const tenant_provider = @import("../state/tenant_provider.zig");
const fleet_telemetry_store = @import("../state/fleet_telemetry_store.zig");
const otel_traces = @import("../observability/otel_traces.zig");
const semconv = @import("../observability/semconv.zig");
const trace = @import("../observability/trace.zig");
const balance_policy = @import("../config/balance_policy.zig");
const COMMIT_FAIL_EVENT = "commit_fail";
const ROLLBACK_FAIL_EVENT = "rollback_fail";

const log = logging.scoped(.fleet_metering);

/// Per-event context shared by the gate, both debits, and post-execution
/// telemetry. Posture and model come from the resolver; everything else
/// flows through from the worker.
const S_COMMIT = "COMMIT";

pub const PreflightContext = struct {
    workspace_id: []const u8,
    fleet_id: []const u8,
    event_id: []const u8,
    posture: tenant_provider.Mode,
    provider: []const u8,
    model: []const u8,
};

pub const DebitOutcome = union(enum) {
    /// Debit + telemetry both committed. Nanos drained on this charge.
    deducted: i64,
    /// Balance < nanos. Tenant balance unchanged. Caller marks gate_blocked.
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
    provider: []const u8,
    model: []const u8,
    policy: balance_policy.Policy,
) bool {
    if (policy != .stop) return true;

    const conn = pool.acquire() catch |err| {
        log.warn("gate_acquire_fail", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .tenant_id = tenant_id, .err = @errorName(err) });
        return true;
    };
    defer pool.release(conn);

    const billing = (tenant_billing.getBilling(conn, alloc, tenant_id) catch |err| {
        log.warn("gate_billing_load_fail", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .tenant_id = tenant_id, .err = @errorName(err) });
        return true;
    }) orelse return true;
    defer alloc.free(@constCast(billing.grant_source));

    const receive = tenant_billing.computeReceiveCharge(posture);
    // Prices the estimate on the connection this gate already holds, so the
    // floor is sized against the live catalogue generation. A failure here — a
    // DB fault like the two above it, or `error.ModelNotPriced` when the
    // catalogue no longer carries the fleet's model — takes the same fail-OPEN
    // answer this gate documents: an ESTIMATE is not a charge, so an
    // unpriceable rate must not turn it into a lease refusal. The charge itself
    // is priced later, at renew/settle, where it fails closed instead.
    const stage = billing_rates.computeStageCharge(
        conn,
        provider,
        posture,
        model,
        0, // elapsed_ms: zero at lease issue — this gate sizes the token-estimate
        // floor only; the run fee accrues per renewal once the fleet is running.
        tenant_billing.ESTIMATE_FLOOR_INPUT_TOKENS,
        0,
        tenant_billing.ESTIMATE_FLOOR_OUTPUT_TOKENS,
        // This tenant's own trial boundary, already loaded above — pricing is
        // per tenant, so one account's trial ending cannot change another's gate.
        billing.free_trial_ends_at_ms,
    ) catch |err| {
        log.warn("gate_stage_estimate_fail", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .tenant_id = tenant_id, .err = @errorName(err) });
        return true;
    };
    return billing.balance_nanos >= (receive + stage);
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
    const nanos = tenant_billing.computeReceiveCharge(ctx.posture);
    return debitAndInsert(pool, alloc, tenant_id, ctx, .receive, nanos, policy);
}

const NANOS_PER_MILLI: u64 = 1_000_000;
const NANOS_PER_SECOND: u64 = 1_000_000_000;
const MILLIS_PER_SECOND: u64 = 1_000;
/// One week. A runner-reported wall time beyond this is not a believable run
/// length, and an unbounded end timestamp would corrupt the trace timeline.
const MAX_SPAN_SECONDS: u64 = 604_800;

/// Emit the `fleet.delivery` span for a finished run. It stays a **custom**
/// control-plane observation rather than a GenAI client span: the runner
/// produces no span and propagates no trace context, so this process cannot
/// honestly claim to be one half of a distributed agent trace. Its attributes
/// are standard where the fact is standard (operation, agent, provider, model,
/// usage) and `agentsfleet.*`-namespaced where the fact is ours.
///
/// The stage row's nanos + token counts are owned by the renewal/settle CTE now
/// (accumulated per slice), so this records no DB row — it is observability
/// only. Fire-and-forget; a non-positive epoch is skipped.
pub fn emitDeliverySpan(
    tenant_id: []const u8,
    ctx: PreflightContext,
    token_count_input: u64,
    token_count_output: u64,
    wall_ms: u64,
    epoch_wall_time_ms: i64,
) void {
    if (epoch_wall_time_ms <= 0) {
        log.warn("skip_delivery_span", .{ .reason = "non_positive_epoch", .fleet_id = ctx.fleet_id });
        return;
    }
    const start_ns: u64 = @as(u64, @intCast(epoch_wall_time_ms)) * NANOS_PER_MILLI;
    const wall_seconds_capped: u64 = @min(wall_ms / MILLIS_PER_SECOND, MAX_SPAN_SECONDS);
    const end_ns: u64 = start_ns + wall_seconds_capped * NANOS_PER_SECOND;
    const tctx = trace.TraceContext.generate();
    var span = otel_traces.buildSpan(tctx, semconv.SPAN_FLEET_DELIVERY, .internal, start_ns, end_ns);
    appendDeliveryAttrs(&span, tenant_id, ctx, token_count_input, token_count_output);
    otel_traces.enqueueSpan(span);
}

/// The delivery span's attribute set. Prompt text, response bodies, and
/// credentials are absent by construction — nothing here reads them.
fn appendDeliveryAttrs(
    span: *otel_traces.SpanEntry,
    tenant_id: []const u8,
    ctx: PreflightContext,
    token_count_input: u64,
    token_count_output: u64,
) void {
    _ = otel_traces.addAttr(span, semconv.ATTR_OPERATION_NAME, semconv.OPERATION_INVOKE_AGENT);
    // The fleet IS the agent in this product — one fleet, one agent identity.
    _ = otel_traces.addAttr(span, semconv.ATTR_AGENT_ID, ctx.fleet_id);
    if (semconv.normalizeProvider(ctx.provider)) |known| {
        _ = otel_traces.addAttr(span, semconv.ATTR_PROVIDER_NAME, known);
    }
    _ = otel_traces.addAttr(span, semconv.ATTR_REQUEST_MODEL, ctx.model);
    _ = otel_traces.addIntAttr(span, semconv.ATTR_USAGE_INPUT_TOKENS, saturatingSigned(token_count_input));
    _ = otel_traces.addIntAttr(span, semconv.ATTR_USAGE_OUTPUT_TOKENS, saturatingSigned(token_count_output));
    _ = otel_traces.addAttr(span, semconv.ATTR_EXECUTION_POSTURE, ctx.posture.label());
    _ = otel_traces.addAttr(span, semconv.ATTR_WORKSPACE_ID, ctx.workspace_id);
    _ = otel_traces.addAttr(span, semconv.ATTR_TENANT_ID, tenant_id);
    _ = otel_traces.addAttr(span, semconv.ATTR_EVENT_ID, ctx.event_id);
}

/// Token counts are runner-controlled `u64`. Saturate rather than `@intCast`,
/// which traps in ReleaseSafe past `i64::MAX` and would abort the daemon over a
/// telemetry value.
fn saturatingSigned(value: u64) i64 {
    return std.math.cast(i64, value) orelse std.math.maxInt(i64);
}

// ── Internal helpers ─────────────────────────────────────────────────────

fn debitAndInsert(
    pool: *pg.Pool,
    alloc: Allocator,
    tenant_id: []const u8,
    ctx: PreflightContext,
    charge_type: fleet_telemetry_store.ChargeType,
    nanos: i64,
    policy: balance_policy.Policy,
) DebitOutcome {
    const conn = pool.acquire() catch |err| {
        log.warn("acquire_fail", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = ctx.fleet_id, .err = @errorName(err) });
        return .{ .db_error = {} };
    };
    defer pool.release(conn);

    _ = conn.exec("BEGIN", .{}) catch |err| {
        log.warn("begin_fail", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = ctx.fleet_id, .err = @errorName(err) });
        return .{ .db_error = {} };
    };
    var tx_open = true;
    defer if (tx_open) {
        conn.rollback() catch |err| log.warn(ROLLBACK_FAIL_EVENT, .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
    };

    if (nanos > 0) {
        _ = tenant_billing.debit(conn, tenant_id, nanos) catch |err| switch (err) {
            error.CreditExhausted => {
                _ = tenant_billing.markExhausted(conn, tenant_id) catch |mark_err| {
                    log.warn("mark_exhausted_fail", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .fleet_id = ctx.fleet_id, .tenant_id = tenant_id, .err = @errorName(mark_err) });
                };
                _ = conn.exec(S_COMMIT, .{}) catch |commit_err| log.warn(COMMIT_FAIL_EVENT, .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(commit_err) });
                tx_open = false;
                onExhaustedDebit(ctx.fleet_id, tenant_id, charge_type, nanos, policy);
                return .{ .exhausted = {} };
            },
            error.TenantBillingMissing => {
                conn.rollback() catch |rollback_err| log.warn(ROLLBACK_FAIL_EVENT, .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(rollback_err) });
                tx_open = false;
                log.err("missing_tenant_billing", .{
                    .error_code = ec.ERR_INTERNAL_OPERATION_FAILED,
                    .fleet_id = ctx.fleet_id,
                    .tenant_id = tenant_id,
                    .workspace_id = ctx.workspace_id,
                    .msg = "starter grant was never inserted for this tenant",
                });
                return .{ .missing_tenant_billing = {} };
            },
            else => {
                conn.rollback() catch |rollback_err| log.warn(ROLLBACK_FAIL_EVENT, .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(rollback_err) });
                tx_open = false;
                log.warn("debit_fail", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = ctx.fleet_id, .tenant_id = tenant_id, .err = @errorName(err) });
                return .{ .db_error = {} };
            },
        };
    }

    fleet_telemetry_store.insertTelemetry(conn, alloc, .{
        .tenant_id = tenant_id,
        .workspace_id = ctx.workspace_id,
        .fleet_id = ctx.fleet_id,
        .event_id = ctx.event_id,
        .charge_type = charge_type,
        .posture = ctx.posture,
        .model = ctx.model,
        .credit_deducted_nanos = nanos,
        .token_count_input = null,
        .token_count_output = null,
        .wall_ms = null,
        .recorded_at = clock.nowMillis(),
    }) catch |err| {
        conn.rollback() catch |rb_err| log.warn(ROLLBACK_FAIL_EVENT, .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(rb_err) });
        tx_open = false;
        log.warn("telemetry_insert_fail", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .fleet_id = ctx.fleet_id, .event_id = ctx.event_id, .charge_type = charge_type.label(), .err = @errorName(err) });
        return .{ .db_error = {} };
    };

    _ = conn.exec(S_COMMIT, .{}) catch |err| {
        log.warn(COMMIT_FAIL_EVENT, .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .fleet_id = ctx.fleet_id, .err = @errorName(err) });
        return .{ .db_error = {} };
    };
    tx_open = false;

    log.debug("debit", .{ .charge_type = charge_type.label(), .tenant_id = tenant_id, .event_id = ctx.event_id, .nanos = nanos });
    return .{ .deducted = nanos };
}

fn onExhaustedDebit(
    fleet_id: []const u8,
    tenant_id: []const u8,
    charge_type: fleet_telemetry_store.ChargeType,
    nanos: i64,
    policy: balance_policy.Policy,
) void {
    log.debug("exhausted", .{
        .fleet_id = fleet_id,
        .tenant_id = tenant_id,
        .charge_type = charge_type.label(),
        .nanos_attempted = nanos,
        .policy = policy.label(),
    });
}

test {
    _ = @import("metering_test.zig");
}
