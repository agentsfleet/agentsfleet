const std = @import("std");
const httpz = @import("httpz");
const state = @import("../../../state/machine.zig");
const policy = @import("../../../state/policy.zig");
const obs_log = @import("../../../observability/logging.zig");
const posthog_events = @import("../../../observability/posthog_events.zig");
const common = @import("../common.zig");
const id_format = @import("../../../types/id_format.zig");
const error_codes = @import("../../../errors/codes.zig");
const billing_runtime = @import("../../../state/billing_runtime.zig");
const log = std.log.scoped(.http);

const queue_unavailable_code = error_codes.ERR_QUEUE_UNAVAILABLE;
const queue_unavailable_message = "Queue unavailable";

pub fn handleRetryRun(ctx: *common.Context, req: *httpz.Request, res: *httpz.Response, run_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch |err| {
        common.writeAuthError(res, req_id, err);
        return;
    };

    if (!id_format.isSupportedRunId(run_id)) {
        common.errorResponse(res, error_codes.ERR_UUIDV7_INVALID_ID_SHAPE, "Invalid run_id format", req_id);
        return;
    }

    const Req = struct {
        reason: []const u8,
        retry_token: []const u8,
    };

    const body = req.body() orelse {
        common.errorResponse(res, error_codes.ERR_INVALID_REQUEST, "Request body required", req_id);
        return;
    };
    const parsed = std.json.parseFromSlice(Req, alloc, body, .{}) catch {
        common.errorResponse(res, error_codes.ERR_INVALID_REQUEST, "Malformed JSON", req_id);
        return;
    };
    defer parsed.deinit();

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
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
        const result = alloc.dupe(u8, wid) catch @as([]const u8, "");
        wq.drain() catch {};
        break :blk result;
    };

    if (workspace_id_for_policy.len > 0 and !common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id_for_policy)) {
        common.errorResponse(res, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    const current = state.getRunState(conn, run_id) catch |err| switch (err) {
        state.TransitionError.RunNotFound => {
            common.errorResponse(res, error_codes.ERR_RUN_NOT_FOUND, "Run not found", req_id);
            return;
        },
        else => {
            common.internalDbError(res, req_id);
            return;
        },
    };

    if (!current.state.isRetryable()) {
        common.errorResponse(res, error_codes.ERR_INVALID_STATE_TRANSITION, "Run is not in a retryable state", req_id);
        return;
    }

    policy.recordPolicyEvent(conn, workspace_id_for_policy, run_id, .sensitive, .allow, "m1.retry_run", "api") catch |err| {
        obs_log.logWarnErr(.http, err, "run.policy_event_insert_fail run_id={s}", .{run_id});
    };

    const now_ms = std.time.milliTimestamp();
    _ = conn.exec(
        "UPDATE runs SET state = 'SPEC_QUEUED', request_id = $1, updated_at = $2 WHERE run_id = $3",
        .{ req_id, now_ms, run_id },
    ) catch {
        common.internalDbError(res, req_id);
        return;
    };

    const transition_id = id_format.generateTransitionId(alloc) catch {
        common.internalDbError(res, req_id);
        return;
    };
    defer alloc.free(transition_id);
    _ = conn.exec(
        \\INSERT INTO run_transitions (id, run_id, attempt, state_from, state_to, actor, reason_code, notes, ts)
        \\VALUES ($1, $2, $3, $4, 'SPEC_QUEUED', $7, 'MANUAL_RETRY', $5, $6)
    , .{
        transition_id,
        run_id,
        @as(i32, @intCast(current.attempt)),
        current.state.label(),
        parsed.value.reason,
        now_ms,
        billing_runtime.LEDGER_ACTOR_ORCHESTRATOR,
    }) catch {
        common.internalDbError(res, req_id);
        return;
    };

    log.info("run.retried run_id={s} reason={s}", .{ run_id, parsed.value.reason });
    ctx.queue.xaddRun(run_id, current.attempt + 1, workspace_id_for_policy) catch |err| {
        obs_log.logWarnErr(.http, err, "run.queue_enqueue_fail run_id={s}", .{run_id});
        common.compensateRetryQueueFailure(conn, run_id, current.state.label(), now_ms);
        common.errorResponse(res, queue_unavailable_code, queue_unavailable_message, req_id);
        return;
    };
    posthog_events.trackRunRetried(
        ctx.posthog,
        posthog_events.distinctIdOrSystem(principal.user_id orelse ""),
        run_id,
        workspace_id_for_policy,
        current.attempt + 1,
        req_id,
    );

    common.writeJson(res, .accepted, .{
        .run_id = run_id,
        .state = "SPEC_QUEUED",
        .attempt = current.attempt,
        .request_id = req_id,
    });
}
