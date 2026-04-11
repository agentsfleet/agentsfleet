//! M28_001 §5.0: Billing summary handler — STUBBED after M10_001.
//! GET /v1/workspaces/:id/billing/summary?period=30d
//!
//! M10_001: billing.usage_ledger and scoring.agent_run_scores tables dropped.
//! Returns zero-valued summary until M15_001 wires zombie credit metering.

const std = @import("std");
const httpz = @import("httpz");
const common = @import("common.zig");
const workspace_guards = @import("../workspace_guards.zig");

const log = std.log.scoped(.http);
const API_ACTOR = "api";

/// Parse period query param: "7d", "30d", "90d". Default 30.
fn parsePeriodDays(raw: ?[]const u8) u32 {
    const val = raw orelse return 30;
    if (std.mem.eql(u8, val, "7d")) return 7;
    if (std.mem.eql(u8, val, "30d")) return 30;
    if (std.mem.eql(u8, val, "90d")) return 90;
    return 30;
}

pub fn handleGetWorkspaceBillingSummary(ctx: *common.Context, req: *httpz.Request, res: *httpz.Response, workspace_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch |err| {
        common.writeAuthError(res, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(res, req_id, workspace_id, "workspace_id")) return;

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    const actor = principal.user_id orelse API_ACTOR;
    const access = workspace_guards.enforce(res, req_id, conn, alloc, principal, workspace_id, actor, .{
        .minimum_role = .operator,
    }) orelse return;
    defer access.deinit(alloc);

    const qs = req.query() catch null;
    const period_days = parsePeriodDays(if (qs) |q| q.get("period") else null);
    const now_ms = std.time.milliTimestamp();
    const period_ms: i64 = @as(i64, @intCast(period_days)) * 24 * 60 * 60 * 1000;
    const period_start_ms = now_ms - period_ms;

    // M10_001: usage_ledger dropped. Return zero-valued summary.
    // M15_001 will wire zombie credit metering and populate real data.
    log.info("billing_summary.ok workspace_id={s} period={d}d total=0 (stub)", .{ workspace_id, period_days });

    common.writeJson(res, .ok, .{
        .workspace_id = workspace_id,
        .period_days = period_days,
        .period_start_ms = period_start_ms,
        .period_end_ms = now_ms,
        .completed = .{ .count = @as(i64, 0), .agent_seconds = @as(i64, 0) },
        .non_billable = .{ .count = @as(i64, 0) },
        .non_billable_score_gated = .{ .count = @as(i64, 0), .avg_score = @as(i64, 0) },
        .total_runs = @as(i64, 0),
        .request_id = req_id,
    });
}
