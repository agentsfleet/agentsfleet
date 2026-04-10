//! M17_001 §3: POST /v1/runs/{run_id}:cancel
//! Publishes a Redis cancellation signal (run:cancel:{run_id}, TTL 1h).
//! Returns 409 if run is already terminal; 200 on success.

const std = @import("std");
const httpz = @import("httpz");
const common = @import("../common.zig");
const state_machine = @import("../../../state/machine.zig");
const types = @import("../../../types.zig");
const error_codes = @import("../../../errors/codes.zig");
const obs_log = @import("../../../observability/logging.zig");
const queue_consts = @import("../../../queue/constants.zig");

const log = std.log.scoped(.http);

const CANCEL_TTL_SECONDS: u32 = 3600;

pub fn handleCancelRun(ctx: *common.Context, req: *httpz.Request, res: *httpz.Response, run_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();

    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch |err| {
        common.writeAuthErrorWithTracking(res, req_id, err, ctx.posthog);
        return;
    };

    if (!common.beginApiRequest(ctx)) {
        common.errorResponse(res, error_codes.ERR_API_SATURATED, "Server overloaded; retry shortly", req_id);
        return;
    }
    defer common.endApiRequest(ctx);

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    // Fetch current state to validate workspace access and terminal check.
    var run_q = conn.query(
        "SELECT state, workspace_id FROM core.runs WHERE run_id = $1",
        .{run_id},
    ) catch {
        common.internalDbError(res, req_id);
        return;
    };
    defer run_q.deinit();

    const row = run_q.next() catch null orelse {
        common.errorResponse(res, error_codes.ERR_RUN_NOT_FOUND, "Run not found", req_id);
        return;
    };
    const state_str = alloc.dupe(u8, row.get([]u8, 0) catch "") catch "";
    const ws_id = alloc.dupe(u8, row.get([]u8, 1) catch "") catch "";
    run_q.drain() catch {};

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, ws_id)) {
        common.errorResponse(res, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    const current = types.RunState.fromStr(state_str) catch {
        common.internalOperationError(res, "Unknown run state", req_id);
        return;
    };

    // M17_001 §3.4: reject cancel if already terminal or in a state where the
    // gate loop consumer is no longer running (BLOCKED, PR_OPENED, NOTIFIED).
    // In those states the Redis signal would expire without any consumer.
    if (current.isTerminal() or current == .BLOCKED or current == .PR_OPENED or current == .NOTIFIED) {
        common.errorResponse(res, error_codes.ERR_RUN_ALREADY_TERMINAL, "Run is already in a terminal state", req_id);
        return;
    }

    // M17_001 §3.1: publish cancel signal to Redis with 1h TTL.
    const redis = ctx.queue;
    const key = std.fmt.allocPrint(alloc, "{s}{s}", .{ queue_consts.cancel_key_prefix, run_id }) catch {
        common.internalOperationError(res, "Key allocation failed", req_id);
        return;
    };
    redis.setEx(key, "1", CANCEL_TTL_SECONDS) catch |err| {
        obs_log.logWarnErr(.http, err, "cancel.redis_setex_fail run_id={s} error_code={s}", .{ run_id, error_codes.ERR_RUN_CANCEL_SIGNAL_FAILED });
        common.errorResponse(res, error_codes.ERR_RUN_CANCEL_SIGNAL_FAILED, "Failed to publish cancel signal", req_id);
        return;
    };

    log.info("run.cancel_signal_published run_id={s} workspace_id={s}", .{ run_id, ws_id });
    common.writeJson(res, .ok, .{
        .run_id = run_id,
        .status = "cancel_requested",
        .request_id = req_id,
    });
}
