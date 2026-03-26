const std = @import("std");
const zap = @import("zap");
const policy = @import("../../state/policy.zig");
const obs_log = @import("../../observability/logging.zig");
const error_codes = @import("../../errors/codes.zig");
const common = @import("common.zig");
const workspace_guards = @import("../workspace_guards.zig");
const log = std.log.scoped(.http);
const API_ACTOR = "api";

pub fn handlePauseWorkspace(ctx: *common.Context, r: zap.Request, workspace_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, r, ctx) catch |err| {
        common.writeAuthError(r, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(r, req_id, workspace_id, "workspace_id")) return;

    const Req = struct {
        pause: bool,
        reason: []const u8,
        version: i64,
    };

    const body = r.body orelse {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Request body required", req_id);
        return;
    };
    if (!common.checkBodySize(r, body, req_id)) return;
    const parsed = std.json.parseFromSlice(Req, alloc, body, .{}) catch {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Malformed JSON", req_id);
        return;
    };
    defer parsed.deinit();

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(r, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    const actor = principal.user_id orelse API_ACTOR;
    const access = workspace_guards.enforce(r, req_id, conn, alloc, principal, workspace_id, actor, .{
        .minimum_role = .operator,
    }) orelse return;
    defer access.deinit(alloc);

    policy.recordPolicyEvent(conn, workspace_id, null, .sensitive, .allow, "m1.pause_workspace", "api") catch |err| {
        obs_log.logWarnErr(.http, err, "workspace.policy_event_insert_fail workspace_id={s}", .{workspace_id});
    };

    const now_ms = std.time.milliTimestamp();
    var upd = conn.query(
        \\UPDATE workspaces
        \\SET paused = $1, paused_reason = $2, version = version + 1, updated_at = $3
        \\WHERE workspace_id = $4 AND version = $5
        \\RETURNING version
    , .{
        parsed.value.pause,
        if (parsed.value.pause) parsed.value.reason else null,
        now_ms,
        workspace_id,
        parsed.value.version,
    }) catch {
        common.internalDbError(r, req_id);
        return;
    };
    defer upd.deinit();

    const row = upd.next() catch null orelse {
        common.errorResponse(r, .conflict, error_codes.ERR_WORKSPACE_NOT_FOUND, "Workspace not found or version conflict", req_id);
        return;
    };

    const new_version = row.get(i64, 0) catch 0;
    upd.drain() catch {};
    log.info("workspace.pause_updated pause={} workspace_id={s}", .{ parsed.value.pause, workspace_id });

    common.writeJson(r, .ok, .{
        .workspace_id = workspace_id,
        .paused = parsed.value.pause,
        .version = new_version,
        .request_id = req_id,
    });
}

pub fn handleSyncSpecs(ctx: *common.Context, r: zap.Request, workspace_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, r, ctx) catch |err| {
        common.writeAuthError(r, req_id, err);
        return;
    };
    if (!common.requireUuidV7Id(r, req_id, workspace_id, "workspace_id")) return;

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(r, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    const actor = principal.user_id orelse API_ACTOR;
    const access = workspace_guards.enforce(r, req_id, conn, alloc, principal, workspace_id, actor, .{
        .credit_policy = .execution_required,
    }) orelse return;
    defer access.deinit(alloc);

    const billing_state = access.billing_state orelse {
        common.internalOperationError(r, "billing_state missing after execution guard", req_id);
        return;
    };

    var ws = conn.query(
        "SELECT repo_url, default_branch FROM workspaces WHERE workspace_id = $1",
        .{workspace_id},
    ) catch {
        common.internalDbError(r, req_id);
        return;
    };
    defer ws.deinit();

    const ws_row = ws.next() catch null orelse {
        common.errorResponse(r, .not_found, error_codes.ERR_WORKSPACE_NOT_FOUND, "Workspace not found", req_id);
        return;
    };

    _ = ws_row;
    ws.drain() catch {};

    var count_result = conn.query(
        "SELECT COUNT(*) FROM specs WHERE workspace_id = $1 AND status = 'pending'",
        .{workspace_id},
    ) catch {
        common.internalDbError(r, req_id);
        return;
    };
    defer count_result.deinit();

    const total_pending: i64 = blk: {
        const crow = (count_result.next() catch null) orelse break :blk @as(i64, 0);
        const v = crow.get(i64, 0) catch @as(i64, 0);
        count_result.drain() catch {};
        break :blk v;
    };

    const credit = access.credit orelse {
        common.internalOperationError(r, "credit missing after execution guard", req_id);
        return;
    };

    log.info("workspace.sync workspace_id={s} total_pending={d}", .{ workspace_id, total_pending });

    common.writeJson(r, .ok, .{
        .synced_count = @as(i64, 0),
        .total_pending = total_pending,
        .specs = &[_]u8{},
        .plan_tier = billing_state.plan_tier.label(),
        .billing_status = billing_state.billing_status.label(),
        .plan_sku = billing_state.plan_sku,
        .grace_expires_at = billing_state.grace_expires_at,
        .credit_remaining_cents = credit.remaining_credit_cents,
        .credit_currency = credit.currency,
        .request_id = req_id,
    });
}
