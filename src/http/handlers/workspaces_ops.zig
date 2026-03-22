const std = @import("std");
const zap = @import("zap");
const workspace_billing = @import("../../state/workspace_billing.zig");
const workspace_credit = @import("../../state/workspace_credit.zig");
const policy = @import("../../state/policy.zig");
const obs_log = @import("../../observability/logging.zig");
const error_codes = @import("../../errors/codes.zig");
const common = @import("common.zig");
const log = std.log.scoped(.http);

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

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id)) {
        common.errorResponse(r, .forbidden, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

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

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id)) {
        common.errorResponse(r, .forbidden, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    const billing_state = workspace_billing.reconcileWorkspaceBilling(conn, alloc, workspace_id, std.time.milliTimestamp(), "api") catch |err| {
        if (workspace_billing.errorCode(err)) |code| {
            common.errorResponse(r, .internal_server_error, code, workspace_billing.errorMessage(err) orelse "Workspace billing failure", req_id);
            return;
        }
        common.internalOperationError(r, "Failed to reconcile workspace billing state", req_id);
        return;
    };
    defer alloc.free(billing_state.plan_sku);
    defer if (billing_state.subscription_id) |v| alloc.free(v);
    const credit = workspace_credit.enforceExecutionAllowed(conn, alloc, workspace_id, billing_state.plan_tier) catch |err| {
        if (workspace_credit.errorCode(err)) |code| {
            common.errorResponse(r, .forbidden, code, workspace_credit.errorMessage(err) orelse "Workspace credit failure", req_id);
            return;
        }
        common.internalOperationError(r, "Failed to validate workspace credit balance", req_id);
        return;
    };
    defer alloc.free(credit.currency);

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
