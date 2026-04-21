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

const log = std.log.scoped(.zombie_metering);

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
        else => return .{ .db_error = {} },
    };
    _ = result;
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
            log.info("metering.exhausted zombie_id={s} tenant_id={s}", .{ zombie_id, tenant_id });
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

test {
    _ = @import("metering_test.zig");
    _ = @import("metering_delivery_telemetry_test.zig");
}
