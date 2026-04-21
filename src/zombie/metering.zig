//! Zombie tenant-billing metering — post-execution debit.
//!
//! Called by the zombie event loop after `deliverEvent()` succeeds and before
//! XACK. Non-blocking: any DB failure returns `.db_error` and the event loop
//! still XACKs so the message is not redelivered.
//!
//! Per-workspace credit state is gone; the worker debits the single
//! `billing.tenant_billing` row shared by all workspaces the tenant owns. The
//! worker resolves `tenant_id` once via `workspace_id → core.workspaces.tenant_id`.
//!
//! MVP note: replay idempotency is out of scope — without an audit table, a
//! redelivered event can double-bill. Acceptable pre-alpha; revisit when
//! Stripe wires in.

const std = @import("std");
const pg = @import("pg");
const Allocator = std.mem.Allocator;

const tenant_billing = @import("../state/tenant_billing.zig");
const zombie_telemetry_store = @import("../state/zombie_telemetry_store.zig");
const otel_traces = @import("../observability/otel_traces.zig");
const trace = @import("../observability/trace.zig");
const activity_stream = @import("activity_stream.zig");
const balance_policy = @import("../config/balance_policy.zig");

const log = std.log.scoped(.zombie_metering);

/// One day in ms — rate-limit window for the recurring `balance_exhausted`
/// event under policy `warn`.
const WARN_RATE_LIMIT_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;

pub const ExecutionUsage = struct {
    zombie_id: []const u8,
    workspace_id: []const u8,
    event_id: []const u8,
    agent_seconds: u64,
    token_count: u64,
    /// ms from stage start to first token. 0 if executor did not report.
    time_to_first_token_ms: u64,
    /// Unix epoch ms at the start of deliverEvent(). 0 on gate-blocked paths.
    epoch_wall_time_ms: i64,
};

pub const DeductionResult = union(enum) {
    /// Cents consumed this call.
    deducted: i64,
    /// Scale plan — charge not applicable; no DB writes.
    exempt: void,
    /// Balance insufficient; no cents deducted.
    exhausted: void,
    /// Tenant has no billing row — bootstrap invariant violated. Logged at
    /// `err`, not `warn`, because it's never an expected operational state.
    missing_tenant_billing: void,
    /// Non-fatal DB failure; event loop continues to XACK.
    db_error: void,
};

pub fn deductZombieUsage(
    conn: *pg.Conn,
    tenant_id: []const u8,
    usage: ExecutionUsage,
    plan_tier: tenant_billing.PlanTier,
) DeductionResult {
    if (plan_tier == .scale) return .{ .exempt = {} };

    const debit_cents = tenant_billing.runtimeUsageCostCents(usage.agent_seconds);
    if (debit_cents <= 0) return .{ .deducted = 0 };

    const result = tenant_billing.debit(conn, tenant_id, debit_cents) catch |err| switch (err) {
        error.CreditExhausted => return .{ .exhausted = {} },
        error.TenantBillingMissing => return .{ .missing_tenant_billing = {} },
        else => return .{ .db_error = {} },
    };
    _ = result;
    // Pre-alpha: no audit table, so log every debit at warn with
    // (event_id, tenant_id, cents) — operators can spot-check a logical
    // event_id appearing twice as a double-bill signal until audit lands.
    log.warn("metering.debit tenant_id={s} event_id={s} cents={d}", .{ tenant_id, usage.event_id, debit_cents });
    return .{ .deducted = debit_cents };
}

pub fn recordZombieDelivery(
    pool: *pg.Pool,
    alloc: Allocator,
    workspace_id: []const u8,
    zombie_id: []const u8,
    event_id: []const u8,
    agent_seconds: u64,
    token_count: u64,
    time_to_first_token_ms: u64,
    epoch_wall_time_ms: i64,
    policy: balance_policy.Policy,
) void {
    const conn = pool.acquire() catch |err| {
        log.warn("metering.acquire_fail zombie_id={s} err={s}", .{ zombie_id, @errorName(err) });
        return;
    };
    defer pool.release(conn);

    const tenant_id = tenant_billing.resolveTenantFromWorkspace(conn, alloc, workspace_id) catch |err| {
        log.warn("metering.tenant_lookup_fail zombie_id={s} err={s}", .{ zombie_id, @errorName(err) });
        return;
    };
    defer alloc.free(tenant_id);

    const plan_tier = tenant_billing.getPlanTier(conn, alloc, tenant_id) catch |err| {
        log.warn("metering.plan_tier_fail zombie_id={s} err={s}", .{ zombie_id, @errorName(err) });
        return;
    };

    const result = deductZombieUsage(conn, tenant_id, .{
        .zombie_id = zombie_id,
        .workspace_id = workspace_id,
        .event_id = event_id,
        .agent_seconds = agent_seconds,
        .token_count = token_count,
        .time_to_first_token_ms = time_to_first_token_ms,
        .epoch_wall_time_ms = epoch_wall_time_ms,
    }, plan_tier);

    const deducted_cents: i64 = switch (result) {
        .deducted => |cents| blk: {
            log.debug("metering.deducted zombie_id={s} cents={d}", .{ zombie_id, cents });
            break :blk cents;
        },
        .exempt => blk: {
            log.debug("metering.exempt zombie_id={s} plan=scale", .{zombie_id});
            break :blk 0;
        },
        .exhausted => blk: {
            onExhaustedDebit(conn, alloc, workspace_id, zombie_id, tenant_id, policy);
            break :blk 0;
        },
        .missing_tenant_billing => blk: {
            log.err("metering.missing_tenant_billing zombie_id={s} tenant_id={s} workspace_id={s} — tenant_billing.provisionFreeDefault was never called for this tenant", .{ zombie_id, tenant_id, workspace_id });
            break :blk 0;
        },
        .db_error => blk: {
            log.warn("metering.db_error zombie_id={s} tenant_id={s}", .{ zombie_id, tenant_id });
            break :blk 0;
        },
    };

    if (epoch_wall_time_ms < 0) {
        log.warn("metering.skip_telemetry reason=negative_epoch zombie_id={s}", .{zombie_id});
        return;
    }
    if (epoch_wall_time_ms == 0) return;

    zombie_telemetry_store.insertTelemetry(conn, alloc, .{
        .zombie_id = zombie_id,
        .workspace_id = workspace_id,
        .event_id = event_id,
        .token_count = token_count,
        .time_to_first_token_ms = time_to_first_token_ms,
        .epoch_wall_time_ms = epoch_wall_time_ms,
        .wall_seconds = agent_seconds,
        .plan_tier = plan_tier.label(),
        .credit_deducted_cents = deducted_cents,
        .recorded_at = std.time.milliTimestamp(),
    }) catch |err| {
        log.warn("metering.telemetry_insert_fail zombie_id={s} err={s}", .{ zombie_id, @errorName(err) });
    };

    {
        const start_ns: u64 = @as(u64, @intCast(epoch_wall_time_ms)) * 1_000_000;
        const capped_seconds: u64 = @min(agent_seconds, 604_800);
        const end_ns: u64 = start_ns + capped_seconds * 1_000_000_000;
        const tctx = trace.TraceContext.generate();
        var span = otel_traces.buildSpan(tctx, "zombie.delivery", start_ns, end_ns);
        _ = otel_traces.addAttr(&span, "zombie_id", zombie_id);
        _ = otel_traces.addAttr(&span, "workspace_id", workspace_id);
        _ = otel_traces.addAttr(&span, "tenant_id", tenant_id);
        _ = otel_traces.addAttr(&span, "event_id", event_id);
        _ = otel_traces.addAttr(&span, "plan_tier", plan_tier.label());
        var cnt_buf: [24]u8 = undefined;
        const cnt_str = std.fmt.bufPrint(&cnt_buf, "{d}", .{token_count}) catch "0";
        _ = otel_traces.addAttr(&span, "token_count", cnt_str);
        otel_traces.enqueueSpan(span);
    }
}

/// Called on the `exhausted` branch of `deductZombieUsage`. Stamps
/// `balance_exhausted_at` (idempotent), emits the one-shot first-debit
/// activity event on transition, and — under policy `warn` — emits the
/// rate-limited recurring `balance_exhausted` event (1/tenant/day).
/// Fire-and-forget: all DB failures log and continue so the zombie keeps
/// running.
fn onExhaustedDebit(
    conn: *pg.Conn,
    alloc: Allocator,
    workspace_id: []const u8,
    zombie_id: []const u8,
    tenant_id: []const u8,
    policy: balance_policy.Policy,
) void {
    const transitioned = tenant_billing.markExhausted(conn, tenant_id) catch |err| {
        log.warn("metering.mark_exhausted_fail tenant_id={s} err={s}", .{ tenant_id, @errorName(err) });
        return;
    };

    if (transitioned) {
        log.info("metering.balance_exhausted_first_debit zombie_id={s} tenant_id={s}", .{ zombie_id, tenant_id });
        activity_stream.logEventOnConn(conn, alloc, .{
            .zombie_id = zombie_id,
            .workspace_id = workspace_id,
            .event_type = activity_stream.EVT_BALANCE_EXHAUSTED_FIRST_DEBIT,
            .detail = tenant_id,
        });
    } else {
        log.info("metering.exhausted zombie_id={s} tenant_id={s} policy={s}", .{ zombie_id, tenant_id, policy.label() });
    }

    if (policy == .warn) {
        const since_ms = std.time.milliTimestamp() - WARN_RATE_LIMIT_WINDOW_MS;
        if (!activity_stream.hasRecentActivityEventOnConn(
            conn,
            workspace_id,
            activity_stream.EVT_BALANCE_EXHAUSTED,
            since_ms,
        )) {
            activity_stream.logEventOnConn(conn, alloc, .{
                .zombie_id = zombie_id,
                .workspace_id = workspace_id,
                .event_type = activity_stream.EVT_BALANCE_EXHAUSTED,
                .detail = tenant_id,
            });
        }
    }
}

/// Pre-claim gate. Resolves tenant_id from workspace_id, loads
/// `balance_exhausted_at`, and returns true iff the policy is `stop` AND
/// the row is marked exhausted. Callers that receive `true` MUST skip
/// `deliverEvent` and write a `balance_gate_blocked` activity event.
///
/// Any DB failure returns false (fail-open) so the gate never turns into
/// an availability incident. Under policy `continue`/`warn` this function
/// short-circuits with `false` without any DB work — the hot path stays
/// unchanged when the feature isn't enabled.
pub fn shouldBlockDelivery(
    pool: *pg.Pool,
    alloc: Allocator,
    workspace_id: []const u8,
    zombie_id: []const u8,
    policy: balance_policy.Policy,
) bool {
    if (policy != .stop) return false;

    const conn = pool.acquire() catch |err| {
        log.warn("gate.acquire_fail zombie_id={s} err={s}", .{ zombie_id, @errorName(err) });
        return false;
    };
    defer pool.release(conn);

    const tenant_id = tenant_billing.resolveTenantFromWorkspace(conn, alloc, workspace_id) catch |err| {
        log.warn("gate.tenant_lookup_fail zombie_id={s} err={s}", .{ zombie_id, @errorName(err) });
        return false;
    };
    defer alloc.free(tenant_id);

    const billing = (tenant_billing.getBilling(conn, alloc, tenant_id) catch |err| {
        log.warn("gate.billing_load_fail zombie_id={s} err={s}", .{ zombie_id, @errorName(err) });
        return false;
    }) orelse return false;
    defer alloc.free(@constCast(billing.plan_tier));
    defer alloc.free(@constCast(billing.plan_sku));
    defer alloc.free(@constCast(billing.grant_source));

    return billing.exhausted_at_ms != null;
}

test {
    _ = @import("metering_test.zig");
    _ = @import("metering_delivery_telemetry_test.zig");
}
