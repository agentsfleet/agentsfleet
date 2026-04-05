//! M28_001 §5.0: Billing summary handler.
//! GET /v1/workspaces/:id/billing/summary?period=30d
//! Returns run counts grouped by lifecycle_event from usage_ledger.

const std = @import("std");
const pg = @import("pg");
const httpz = @import("httpz");
const common = @import("common.zig");
const workspace_guards = @import("../workspace_guards.zig");
const error_codes = @import("../../errors/codes.zig");

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

const SummaryRow = struct {
    lifecycle_event: []const u8,
    run_count: i64,
    total_agent_seconds: i64,
    total_billable_quantity: i64,
};

const ScoreRow = struct {
    avg_score: i64,
};

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

    // Parse period from query string.
    const qs = req.query() catch null;
    const period_days = parsePeriodDays(if (qs) |q| q.get("period") else null);
    const now_ms = std.time.milliTimestamp();
    const period_ms: i64 = @as(i64, @intCast(period_days)) * 24 * 60 * 60 * 1000;
    const period_start_ms = now_ms - period_ms;

    // Query usage_ledger grouped by lifecycle_event.
    // Only finalization rows (source = 'runtime_summary') to avoid double-counting stages.
    var q = conn.query(
        \\SELECT lifecycle_event,
        \\       COUNT(*)::bigint AS run_count,
        \\       COALESCE(SUM(agent_seconds), 0)::bigint AS total_agent_seconds,
        \\       COALESCE(SUM(billable_quantity), 0)::bigint AS total_billable_quantity
        \\FROM billing.usage_ledger
        \\WHERE workspace_id = $1
        \\  AND source = 'runtime_summary'
        \\  AND created_at >= $2
        \\GROUP BY lifecycle_event
        \\ORDER BY lifecycle_event
    , .{ workspace_id, period_start_ms }) catch {
        common.internalOperationError(res, "Failed to query billing summary", req_id);
        return;
    };
    defer {
        q.drain() catch {};
        q.deinit();
    }

    var completed_count: i64 = 0;
    var completed_seconds: i64 = 0;
    var non_billable_count: i64 = 0;
    var score_gated_count: i64 = 0;
    var total_runs: i64 = 0;

    while (true) {
        const row = q.next() catch |err| {
            log.warn("billing_summary.query_fail workspace_id={s} err={s}", .{ workspace_id, @errorName(err) });
            break;
        } orelse break;
        const event = row.get([]u8, 0) catch continue;
        const count = row.get(i64, 1) catch continue;
        const seconds = row.get(i64, 2) catch 0;
        total_runs += count;

        if (std.mem.eql(u8, event, "run_completed")) {
            completed_count = count;
            completed_seconds = seconds;
        } else if (std.mem.eql(u8, event, "run_not_billable")) {
            non_billable_count += count;
        } else if (std.mem.eql(u8, event, "run_not_billable_score_gated")) {
            score_gated_count = count;
        }
    }

    // §5.5: Average score for score-gated runs.
    const avg_gated_score: i64 = blk: {
        if (score_gated_count == 0) break :blk 0;
        var sq = conn.query(
            \\SELECT COALESCE(AVG(s.score), 0)::bigint
            \\FROM billing.usage_ledger u
            \\JOIN scoring.agent_run_scores s ON s.run_id = u.run_id
            \\WHERE u.workspace_id = $1
            \\  AND u.lifecycle_event = 'run_not_billable_score_gated'
            \\  AND u.source = 'runtime_summary'
            \\  AND u.created_at >= $2
        , .{ workspace_id, period_start_ms }) catch break :blk 0;
        defer {
            sq.drain() catch {};
            sq.deinit();
        }
        const srow = sq.next() catch break :blk 0;
        if (srow) |row| {
            break :blk row.get(i64, 0) catch 0;
        }
        break :blk 0;
    };

    log.info("billing_summary.ok workspace_id={s} period={d}d total={d}", .{ workspace_id, period_days, total_runs });

    common.writeJson(res, .ok, .{
        .workspace_id = workspace_id,
        .period_days = period_days,
        .period_start_ms = period_start_ms,
        .period_end_ms = now_ms,
        .completed = .{
            .count = completed_count,
            .agent_seconds = completed_seconds,
        },
        .non_billable = .{
            .count = non_billable_count,
        },
        .non_billable_score_gated = .{
            .count = score_gated_count,
            .avg_score = avg_gated_score,
        },
        .total_runs = total_runs,
        .request_id = req_id,
    });
}
