//! GET /v1/runs/{id}/replay — returns structured gate results for post-mortem replay.
//! Works for both failed and successful runs.

const std = @import("std");
const httpz = @import("httpz");
const common = @import("../common.zig");
const obs_log = @import("../../../observability/logging.zig");
const id_format = @import("../../../types/id_format.zig");
const error_codes = @import("../../../errors/codes.zig");

const log = std.log.scoped(.http);

const GateResultEntry = struct {
    gate_name: []const u8,
    attempt: i32,
    exit_code: i32,
    stdout_tail: []const u8,
    stderr_tail: []const u8,
    wall_ms: i64,
    created_at: i64,
};

const ReplayResponse = struct {
    run_id: []const u8,
    current_state: []const u8,
    gate_results: []const GateResultEntry,
    request_id: []const u8,
};

pub fn handleGetRunReplay(ctx: *common.Context, req: *httpz.Request, res: *httpz.Response, run_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch |err| {
        common.writeAuthError(res, req_id, err);
        return;
    };

    log.debug("run.replay run_id={s}", .{run_id});

    if (!id_format.isSupportedRunId(run_id)) {
        common.errorResponse(res, .bad_request, error_codes.ERR_UUIDV7_INVALID_ID_SHAPE, "Invalid run_id format", req_id);
        return;
    }

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    var run_result = conn.query(
        "SELECT run_id, workspace_id, state FROM runs WHERE run_id = $1",
        .{run_id},
    ) catch {
        common.internalDbError(res, req_id);
        return;
    };
    defer run_result.deinit();

    const row = run_result.next() catch null orelse {
        common.errorResponse(res, .not_found, error_codes.ERR_RUN_NOT_FOUND, "Run not found", req_id);
        return;
    };

    const workspace_id = row.get([]u8, 1) catch "?";
    const run_state = row.get([]u8, 2) catch "?";

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id)) {
        common.errorResponse(res, .forbidden, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    run_result.drain() catch |err| obs_log.logWarnErr(.http, err, "run.replay_drain_fail run_id={s}", .{run_id});

    var gate_results_list: std.ArrayList(GateResultEntry) = .{};

    var gr_result = conn.query(
        \\SELECT gate_name, attempt, exit_code, stdout_tail, stderr_tail, wall_ms, created_at
        \\FROM gate_results WHERE run_id = $1
        \\ORDER BY attempt ASC, created_at ASC
    , .{run_id}) catch {
        common.internalDbError(res, req_id);
        return;
    };
    defer gr_result.deinit();

    while (gr_result.next() catch null) |gr_row| {
        const gate_name = gr_row.get([]u8, 0) catch continue;
        const attempt = gr_row.get(i32, 1) catch 0;
        const exit_code = gr_row.get(i32, 2) catch -1;
        const stdout_tail = gr_row.get([]u8, 3) catch "";
        const stderr_tail = gr_row.get([]u8, 4) catch "";
        const wall_ms = gr_row.get(i64, 5) catch 0;
        const created_at = gr_row.get(i64, 6) catch 0;

        gate_results_list.append(alloc, .{
            .gate_name = gate_name,
            .attempt = attempt,
            .exit_code = exit_code,
            .stdout_tail = stdout_tail,
            .stderr_tail = stderr_tail,
            .wall_ms = wall_ms,
            .created_at = created_at,
        }) catch continue;
    }

    gr_result.drain() catch |err| obs_log.logWarnErr(.http, err, "run.replay_gr_drain_fail run_id={s}", .{run_id});

    common.writeJson(res, .ok, ReplayResponse{
        .run_id = run_id,
        .current_state = run_state,
        .gate_results = gate_results_list.items,
        .request_id = req_id,
    });
}
