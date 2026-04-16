// M12_001: Per-zombie billing summary — same data as workspace summary, scoped
// to one zombie. Shares the response schema so an SDK has a single shape for
// both scopes: workspaces.billing.summary(ws) and workspaces.zombies.billing.summary(ws, z).
//
// GET /v1/workspaces/{ws}/zombies/{zombie_id}/billing/summary?period_days=7|30
//
// Auth: bearer. Workspace scope enforced by common.authorizeWorkspace. Zombie
// ownership enforced by RULE ZWO (getZombieWorkspaceId + workspace match).
//
// Data: zombie_execution_telemetry aggregated over [now - period_days, now].

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;

const common = @import("common.zig");
const hx_mod = @import("hx.zig");
const ec = @import("../../errors/error_registry.zig");
const id_format = @import("../../types/id_format.zig");
const billing_summary_store = @import("../../state/billing_summary_store.zig");

const log = std.log.scoped(.zombie_billing_summary);

/// Parse period_days query param. Accepts "7" or "30"; defaults to 30.
/// Accepting bare integers keeps the param SDK-friendly; the workspace summary
/// handler accepts legacy "7d"/"30d" suffixes too for backwards-compat — the
/// zombie endpoint is new so only the integer form is supported.
pub fn parsePeriodDays(raw: ?[]const u8) u32 {
    const val = raw orelse return 30;
    const n = std.fmt.parseInt(u32, val, 10) catch return 30;
    if (n == 7) return 7;
    if (n == 30) return 30;
    return 30;
}

pub fn innerGetZombieBillingSummary(
    hx: hx_mod.Hx,
    req: *httpz.Request,
    workspace_id: []const u8,
    zombie_id: []const u8,
) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    if (!id_format.isSupportedWorkspaceId(zombie_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "zombie_id must be a valid UUIDv7");
        return;
    }

    const qs = req.query() catch null;
    const period_days = parsePeriodDays(if (qs) |q| q.get("period_days") else null);

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return;
    }

    // RULE ZWO: verify zombie belongs to the path workspace (404 — don't leak existence).
    const zombie_ws_id = common.getZombieWorkspaceId(conn, hx.alloc, zombie_id) orelse {
        hx.fail(ec.ERR_ZOMBIE_NOT_FOUND, ec.MSG_ZOMBIE_NOT_FOUND);
        return;
    };
    if (!std.mem.eql(u8, zombie_ws_id, workspace_id)) {
        hx.fail(ec.ERR_ZOMBIE_NOT_FOUND, ec.MSG_ZOMBIE_NOT_FOUND);
        return;
    }

    const now_ms = std.time.milliTimestamp();
    const period_ms: i64 = @as(i64, @intCast(period_days)) * 24 * 60 * 60 * 1000;
    const period_start_ms = now_ms - period_ms;

    const summary = billing_summary_store.aggregateForZombie(conn, workspace_id, zombie_id, period_start_ms) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    log.info("zombie_billing_summary.ok workspace={s} zombie={s} period={d}d runs={d}", .{
        workspace_id, zombie_id, period_days, summary.total_runs,
    });

    hx.ok(.ok, .{
        .workspace_id = workspace_id,
        .zombie_id = zombie_id,
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

// ── Unit tests ────────────────────────────────────────────────────────────

test "parsePeriodDays: missing value defaults to 30" {
    try std.testing.expectEqual(@as(u32, 30), parsePeriodDays(null));
}

test "parsePeriodDays: '7' returns 7" {
    try std.testing.expectEqual(@as(u32, 7), parsePeriodDays("7"));
}

test "parsePeriodDays: '30' returns 30" {
    try std.testing.expectEqual(@as(u32, 30), parsePeriodDays("30"));
}

test "parsePeriodDays: unsupported integer clamps to 30" {
    try std.testing.expectEqual(@as(u32, 30), parsePeriodDays("14"));
    try std.testing.expectEqual(@as(u32, 30), parsePeriodDays("90"));
}

test "parsePeriodDays: non-integer falls back to 30" {
    try std.testing.expectEqual(@as(u32, 30), parsePeriodDays("7d"));
    try std.testing.expectEqual(@as(u32, 30), parsePeriodDays("abc"));
    try std.testing.expectEqual(@as(u32, 30), parsePeriodDays(""));
}
