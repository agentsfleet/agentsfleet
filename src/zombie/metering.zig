//! Zombie credit metering — post-execution deduction per M15_001.
//!
//! Called by the zombie event loop after `deliverEvent()` succeeds and before
//! XACK. Non-blocking: any DB failure returns `.db_error` and the event loop
//! still XACKs so the message is not redelivered.
//!
//! Idempotency: uses `workspace_credit_store.hasAuditEvent` keyed on
//! metadata_json (which embeds `event_id`) so a crash-recovery replay of the
//! same event deducts 0 cents on the second call.

const std = @import("std");
const pg = @import("pg");
const Allocator = std.mem.Allocator;

const workspace_credit = @import("../state/workspace_credit.zig");
const workspace_credit_store = @import("../state/workspace_credit_store.zig");
const billing_row = @import("../state/workspace_billing/row.zig");
const zombie_telemetry_store = @import("../state/zombie_telemetry_store.zig");
const otel_traces = @import("../observability/otel_traces.zig");
const trace = @import("../observability/trace.zig");

const log = std.log.scoped(.zombie_metering);

const ACTOR = "zombie_event_loop";
const EVENT_TYPE = "CREDIT_DEDUCTED";
const REASON = "runtime_completed";

pub const ExecutionUsage = struct {
    zombie_id: []const u8, // borrowed
    workspace_id: []const u8, // borrowed
    event_id: []const u8, // borrowed — idempotency key embedded in audit metadata
    agent_seconds: u64,
    token_count: u64,
    /// M18_001: ms from stage start to first token. 0 if executor did not report.
    time_to_first_token_ms: u64,
    /// M18_001: Unix epoch ms at the start of deliverEvent(). 0 on gate-blocked paths.
    epoch_wall_time_ms: i64,
};

pub const DeductionResult = union(enum) {
    /// Cents consumed this call. 0 on replay or zero-cost events.
    deducted: i64,
    /// Scale plan — charge not applicable; no DB writes.
    exempt: void,
    /// Credits were already 0; a zero-delta audit row is still written.
    exhausted: i64,
    /// Non-fatal DB failure; event loop continues to XACK.
    db_error: void,
};

/// Deduct credits for one zombie event delivery. Must stay ≤50 lines.
///
/// Idempotency is keyed on `event_id` via `hasAuditForRunId`, NOT on full
/// metadata_json. This matters: on XACK failure the event is redelivered and
/// re-executed, producing a different `agent_seconds` (LLM + tool latency
/// varies between retries). A metadata_json-keyed check would miss and
/// double-bill. Keying on event_id dedupes the logical event regardless of
/// measured duration.
pub fn deductZombieUsage(
    conn: *pg.Conn,
    alloc: Allocator,
    usage: ExecutionUsage,
    plan_tier: billing_row.PlanTier,
) DeductionResult {
    if (plan_tier == .scale) return .{ .exempt = {} };

    const already = workspace_credit_store.hasAuditForRunId(conn, usage.workspace_id, EVENT_TYPE, usage.event_id) catch return .{ .db_error = {} };
    if (already) return .{ .deducted = 0 };

    const pre = workspace_credit.getOrProvisionWorkspaceCredit(conn, alloc, usage.workspace_id) catch return .{ .db_error = {} };
    alloc.free(pre.currency);

    if (pre.remaining_credit_cents <= 0) {
        const debit_cents = workspace_credit.runtimeUsageCostCents(usage.agent_seconds);
        const metadata_json = workspace_credit_store.runtimeDeductionMetadata(alloc, usage.event_id, 0, usage.agent_seconds, debit_cents) catch return .{ .db_error = {} };
        defer alloc.free(metadata_json);
        workspace_credit_store.insertAudit(conn, alloc, usage.workspace_id, EVENT_TYPE, 0, 0, REASON, ACTOR, metadata_json) catch return .{ .db_error = {} };
        return .{ .exhausted = 0 };
    }

    const post = workspace_credit.deductCompletedRuntimeUsage(conn, alloc, usage.workspace_id, usage.event_id, 0, usage.agent_seconds, ACTOR) catch return .{ .db_error = {} };
    alloc.free(post.currency);
    return .{ .deducted = post.consumed_credit_cents - pre.consumed_credit_cents };
}

/// Convenience wrapper invoked by the event loop after successful delivery.
/// Non-fatal: all errors are logged; caller continues to XACK.
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

    const plan_tier = resolvePlanTier(conn, alloc, workspace_id) catch |err| {
        log.warn("metering.plan_tier_fail zombie_id={s} err={s}", .{ zombie_id, @errorName(err) });
        return;
    };

    const result = deductZombieUsage(conn, alloc, .{
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
            log.info("metering.exhausted zombie_id={s} workspace_id={s}", .{ zombie_id, workspace_id });
            break :blk 0;
        },
        .db_error => blk: {
            log.warn("metering.db_error zombie_id={s} workspace_id={s}", .{ zombie_id, workspace_id });
            break :blk 0;
        },
    };

    // M18_001: persist per-delivery telemetry. Non-fatal — DB failure logged, delivery unaffected.
    zombie_telemetry_store.insertTelemetry(conn, alloc, .{
        .zombie_id = zombie_id,
        .workspace_id = workspace_id,
        .event_id = event_id,
        .token_count = token_count,
        .time_to_first_token_ms = time_to_first_token_ms,
        .epoch_wall_time_ms = epoch_wall_time_ms,
        .wall_seconds = agent_seconds,
        .plan_tier = @tagName(plan_tier),
        .credit_deducted_cents = deducted_cents,
        .recorded_at = std.time.milliTimestamp(),
    }) catch |err| {
        log.warn("metering.telemetry_insert_fail zombie_id={s} err={s}", .{ zombie_id, @errorName(err) });
    };

    // M18_001: emit OTel span for Grafana/Tempo per-delivery trace visibility.
    // Root span — zombie delivery has no inbound HTTP traceparent.
    // Skipped when epoch_wall_time_ms=0 (gate-blocked or pre-M18 path).
    if (epoch_wall_time_ms > 0) {
        const start_ns: u64 = @as(u64, @intCast(epoch_wall_time_ms)) * 1_000_000;
        const end_ns: u64 = start_ns + agent_seconds * 1_000_000_000;
        const tctx = trace.TraceContext.generate();
        var span = otel_traces.buildSpan(tctx, "zombie.delivery", start_ns, end_ns);
        _ = otel_traces.addAttr(&span, "zombie_id", zombie_id);
        _ = otel_traces.addAttr(&span, "workspace_id", workspace_id);
        _ = otel_traces.addAttr(&span, "event_id", event_id);
        _ = otel_traces.addAttr(&span, "plan_tier", @tagName(plan_tier));
        var cnt_buf: [24]u8 = undefined;
        const cnt_str = std.fmt.bufPrint(&cnt_buf, "{d}", .{token_count}) catch "0";
        _ = otel_traces.addAttr(&span, "token_count", cnt_str);
        otel_traces.enqueueSpan(span);
    }
}

fn resolvePlanTier(
    conn: *pg.Conn,
    alloc: Allocator,
    workspace_id: []const u8,
) !billing_row.PlanTier {
    var row = (try billing_row.loadStateRow(conn, alloc, workspace_id)) orelse
        return error.WorkspaceBillingStateMissing;
    defer row.deinit(alloc);
    return row.plan_tier;
}

test {
    _ = @import("metering_test.zig");
}
