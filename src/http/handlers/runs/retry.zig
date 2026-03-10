const std = @import("std");
const zap = @import("zap");
const state = @import("../../../state/machine.zig");
const policy = @import("../../../state/policy.zig");
const obs_log = @import("../../../observability/logging.zig");
const common = @import("../common.zig");
const log = std.log.scoped(.http);

const queue_unavailable_code = "QUEUE_UNAVAILABLE";
const queue_unavailable_message = "Queue unavailable";

pub fn handleRetryRun(ctx: *common.Context, r: zap.Request, run_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, r, ctx) catch |err| {
        common.writeAuthError(r, req_id, err);
        return;
    };

    const Req = struct {
        reason: []const u8,
        retry_token: []const u8,
    };

    const body = r.body orelse {
        common.errorResponse(r, .bad_request, "INVALID_REQUEST", "Request body required", req_id);
        return;
    };
    const parsed = std.json.parseFromSlice(Req, alloc, body, .{}) catch {
        common.errorResponse(r, .bad_request, "INVALID_REQUEST", "Malformed JSON", req_id);
        return;
    };
    defer parsed.deinit();

    const conn = ctx.pool.acquire() catch {
        common.errorResponse(r, .service_unavailable, "INTERNAL_ERROR", "Database unavailable", req_id);
        return;
    };
    defer ctx.pool.release(conn);

    const workspace_id_for_policy: []const u8 = blk: {
        var wq = conn.query(
            "SELECT workspace_id FROM runs WHERE run_id = $1",
            .{run_id},
        ) catch break :blk @as([]const u8, "");
        defer wq.deinit();
        const wrow = wq.next() catch null orelse break :blk @as([]const u8, "");
        const wid = wrow.get([]u8, 0) catch break :blk @as([]const u8, "");
        break :blk alloc.dupe(u8, wid) catch @as([]const u8, "");
    };

    if (workspace_id_for_policy.len > 0 and !common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id_for_policy)) {
        common.errorResponse(r, .forbidden, "FORBIDDEN", "Workspace access denied", req_id);
        return;
    }

    const current = state.getRunState(conn, run_id) catch |err| switch (err) {
        state.TransitionError.RunNotFound => {
            common.errorResponse(r, .not_found, "RUN_NOT_FOUND", "Run not found", req_id);
            return;
        },
        else => {
            common.errorResponse(r, .internal_server_error, "INTERNAL_ERROR", "Database error", req_id);
            return;
        },
    };

    if (!current.state.isRetryable()) {
        common.errorResponse(r, .unprocessable_content, "INVALID_STATE_TRANSITION", "Run is not in a retryable state", req_id);
        return;
    }

    policy.recordPolicyEvent(conn, workspace_id_for_policy, run_id, .sensitive, .allow, "m1.retry_run", "api") catch |err| {
        obs_log.logWarnErr(.http, err, "policy event insert failed (non-fatal) run_id={s}", .{run_id});
    };

    const now_ms = std.time.milliTimestamp();
    var r2 = conn.query(
        "UPDATE runs SET state = 'SPEC_QUEUED', request_id = $1, updated_at = $2 WHERE run_id = $3",
        .{ req_id, now_ms, run_id },
    ) catch {
        common.errorResponse(r, .internal_server_error, "INTERNAL_ERROR", "Database error", req_id);
        return;
    };
    r2.deinit();

    var r3 = conn.query(
        \\INSERT INTO run_transitions (run_id, attempt, state_from, state_to, actor, reason_code, notes, ts)
        \\VALUES ($1, $2, $3, 'SPEC_QUEUED', 'orchestrator', 'MANUAL_RETRY', $4, $5)
    , .{
        run_id,                @as(i32, @intCast(current.attempt)),
        current.state.label(), parsed.value.reason,
        now_ms,
    }) catch {
        common.errorResponse(r, .internal_server_error, "INTERNAL_ERROR", "Database error", req_id);
        return;
    };
    r3.deinit();

    log.info("run retried run_id={s} reason={s}", .{ run_id, parsed.value.reason });
    ctx.queue.xaddRun(run_id, current.attempt + 1, workspace_id_for_policy) catch |err| {
        obs_log.logWarnErr(.http, err, "queue enqueue failed for retry run_id={s}", .{run_id});
        common.compensateRetryQueueFailure(conn, run_id, current.state.label(), now_ms);
        common.errorResponse(r, .service_unavailable, queue_unavailable_code, queue_unavailable_message, req_id);
        return;
    };

    common.writeJson(r, .accepted, .{
        .run_id = run_id,
        .state = "SPEC_QUEUED",
        .attempt = current.attempt,
        .request_id = req_id,
    });
}
