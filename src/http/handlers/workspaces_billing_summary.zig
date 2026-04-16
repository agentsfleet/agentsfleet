//! Workspace billing summary handler.
//! GET /v1/workspaces/:id/billing/summary?period=30d (legacy) | ?period_days=30 (new)
//!
//! History:
//!   - M28_001 §5.0 stubbed this to zeros after M10_001 dropped usage_ledger.
//!   - M12_001 wires it to zombie_execution_telemetry so the dashboard has
//!     real counts, agent-seconds, and credit totals. Per-zombie slice lives
//!     in zombie_billing_summary.zig and shares the same aggregation module.

const std = @import("std");
const httpz = @import("httpz");
const common = @import("common.zig");
const workspace_guards = @import("../workspace_guards.zig");
const hx_mod = @import("hx.zig");
const billing_summary_store = @import("../../state/billing_summary_store.zig");

const log = std.log.scoped(.http);
const API_ACTOR = "api";

/// Accept the legacy `?period=7d|30d|90d` param and the new `?period_days=7|30`
/// form. Legacy form is preserved so existing billing/summary consumers do not
/// break; new per-zombie handler uses only the integer form.
pub fn parsePeriodDays(qs_period: ?[]const u8, qs_period_days: ?[]const u8) u32 {
    if (qs_period_days) |raw| {
        const n = std.fmt.parseInt(u32, raw, 10) catch return 30;
        if (n == 7 or n == 30 or n == 90) return n;
        return 30;
    }
    const val = qs_period orelse return 30;
    if (std.mem.eql(u8, val, "7d")) return 7;
    if (std.mem.eql(u8, val, "30d")) return 30;
    if (std.mem.eql(u8, val, "90d")) return 90;
    return 30;
}

pub fn innerGetWorkspaceBillingSummary(hx: hx_mod.Hx, req: *httpz.Request, workspace_id: []const u8) void {
    if (!common.requireUuidV7Id(hx.res, hx.req_id, workspace_id, "workspace_id")) return;

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const actor = hx.principal.user_id orelse API_ACTOR;
    const access = workspace_guards.enforce(hx.res, hx.req_id, conn, hx.alloc, hx.principal, workspace_id, actor, .{
        .minimum_role = .operator,
    }) orelse return;
    defer access.deinit(hx.alloc);

    const qs = req.query() catch null;
    const period_days = parsePeriodDays(
        if (qs) |q| q.get("period") else null,
        if (qs) |q| q.get("period_days") else null,
    );
    const now_ms = std.time.milliTimestamp();
    const period_ms: i64 = @as(i64, @intCast(period_days)) * 24 * 60 * 60 * 1000;
    const period_start_ms = now_ms - period_ms;

    const summary = billing_summary_store.aggregateForWorkspace(conn, workspace_id, period_start_ms) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    log.info("billing_summary.ok workspace_id={s} period={d}d total_runs={d} total_cents={d}", .{
        workspace_id, period_days, summary.total_runs, summary.total_cents,
    });

    hx.ok(.ok, .{
        .workspace_id = workspace_id,
        .period_days = period_days,
        .period_start_ms = period_start_ms,
        .period_end_ms = now_ms,
        .completed = .{ .count = summary.completed_count, .agent_seconds = summary.completed_agent_seconds },
        .non_billable = .{ .count = summary.non_billable_count },
        .non_billable_score_gated = .{ .count = @as(i64, 0), .avg_score = @as(i64, 0) },
        .total_runs = summary.total_runs,
        .total_cents = summary.total_cents,
        .request_id = hx.req_id,
    });
}

test "parsePeriodDays: legacy period=7d|30d|90d" {
    try std.testing.expectEqual(@as(u32, 7), parsePeriodDays("7d", null));
    try std.testing.expectEqual(@as(u32, 30), parsePeriodDays("30d", null));
    try std.testing.expectEqual(@as(u32, 90), parsePeriodDays("90d", null));
}

test "parsePeriodDays: new period_days=7|30|90 wins over legacy" {
    try std.testing.expectEqual(@as(u32, 7), parsePeriodDays("30d", "7"));
    try std.testing.expectEqual(@as(u32, 30), parsePeriodDays(null, "30"));
    try std.testing.expectEqual(@as(u32, 90), parsePeriodDays(null, "90"));
}

test "parsePeriodDays: unrecognized values default to 30" {
    try std.testing.expectEqual(@as(u32, 30), parsePeriodDays(null, null));
    try std.testing.expectEqual(@as(u32, 30), parsePeriodDays("14d", null));
    try std.testing.expectEqual(@as(u32, 30), parsePeriodDays(null, "14"));
    try std.testing.expectEqual(@as(u32, 30), parsePeriodDays(null, "abc"));
}
