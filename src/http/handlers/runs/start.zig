const std = @import("std");
const zap = @import("zap");
const common = @import("../common.zig");
const policy = @import("../../../state/policy.zig");
const metrics = @import("../../../observability/metrics.zig");
const trace_ctx = @import("../../../observability/trace.zig");
const obs_log = @import("../../../observability/logging.zig");
const profile_linkage = @import("../../../audit/profile_linkage.zig");
const id_format = @import("../../../types/id_format.zig");
const error_codes = @import("../../../errors/codes.zig");
const log = std.log.scoped(.http);

const queue_unavailable_code = error_codes.ERR_QUEUE_UNAVAILABLE;
const queue_unavailable_message = "Queue unavailable";

pub fn handleStartRun(ctx: *common.Context, r: zap.Request) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();

    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, r, ctx) catch |err| {
        common.writeAuthError(r, req_id, err);
        return;
    };

    const body = r.body orelse {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Request body required", req_id);
        return;
    };

    const Req = struct {
        workspace_id: []const u8,
        spec_id: []const u8,
        mode: []const u8,
        requested_by: []const u8,
        idempotency_key: []const u8,
    };

    const parsed = std.json.parseFromSlice(Req, alloc, body, .{}) catch {
        common.errorResponse(r, .bad_request, error_codes.ERR_INVALID_REQUEST, "Malformed JSON or missing required fields", req_id);
        return;
    };
    defer parsed.deinit();
    const req = parsed.value;
    if (!common.requireUuidV7Id(r, req_id, req.workspace_id, "workspace_id")) return;
    if (!common.requireUuidV7Id(r, req_id, req.spec_id, "spec_id")) return;

    if (!common.beginApiRequest(ctx)) {
        common.errorResponse(r, .service_unavailable, error_codes.ERR_API_SATURATED, "Server overloaded; retry shortly", req_id);
        return;
    }
    defer common.endApiRequest(ctx);

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(r, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, req.workspace_id)) {
        common.errorResponse(r, .forbidden, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    {
        var ws_check = conn.query(
            "SELECT paused FROM workspaces WHERE workspace_id = $1",
            .{req.workspace_id},
        ) catch {
            common.internalDbError(r, req_id);
            return;
        };
        defer ws_check.deinit();

        const ws_row = ws_check.next() catch null orelse {
            common.errorResponse(r, .not_found, error_codes.ERR_WORKSPACE_NOT_FOUND, "Workspace not found", req_id);
            return;
        };

        const paused = ws_row.get(bool, 0) catch false;
        if (paused) {
            common.errorResponse(r, .conflict, error_codes.ERR_WORKSPACE_PAUSED, "Workspace is paused", req_id);
            return;
        }
    }

    {
        var spec_check = conn.query(
            "SELECT spec_id FROM specs WHERE spec_id = $1 AND workspace_id = $2",
            .{ req.spec_id, req.workspace_id },
        ) catch {
            common.internalDbError(r, req_id);
            return;
        };
        defer spec_check.deinit();

        if (spec_check.next() catch null == null) {
            common.errorResponse(r, .not_found, error_codes.ERR_SPEC_NOT_FOUND, "Spec not found", req_id);
            return;
        }
    }

    const run_id = id_format.generateRunId(alloc) catch |err| {
        obs_log.logWarnErr(.http, err, "error_code={s} run_id generation failed", .{error_codes.ERR_UUIDV7_ID_GENERATION_FAILED});
        common.errorResponse(r, .internal_server_error, error_codes.ERR_UUIDV7_ID_GENERATION_FAILED, "Failed to generate run identifier", req_id);
        return;
    };

    // Parse W3C traceparent header or generate new trace context
    const tc = if (r.getHeader("traceparent")) |tp|
        trace_ctx.TraceContext.fromW3CHeader(tp) orelse trace_ctx.TraceContext.generate()
    else
        trace_ctx.TraceContext.generate();
    const trace_id: []const u8 = tc.traceIdSlice();

    const now_ms = std.time.milliTimestamp();
    const branch = std.fmt.allocPrint(alloc, "zombie/run-{s}", .{run_id}) catch "zombie/run-unknown";

    var insert = conn.query(
        \\INSERT INTO runs
        \\  (run_id, workspace_id, spec_id, tenant_id, state, attempt, mode,
        \\   requested_by, idempotency_key, request_id, trace_id, branch, run_snapshot_version, created_at, updated_at)
        \\SELECT $1, $2, $3, tenant_id, 'SPEC_QUEUED', 1, $4, $5, $6, $7,
        \\       $8, $9, (SELECT wap.profile_version_id FROM workspace_active_profile wap WHERE wap.workspace_id = $2), $10, $10
        \\FROM workspaces WHERE workspace_id = $2
        \\ON CONFLICT (workspace_id, idempotency_key) DO UPDATE
        \\SET updated_at = runs.updated_at
        \\RETURNING run_id, state, attempt, run_snapshot_version, tenant_id, (xmax = 0) AS inserted
    , .{ run_id, req.workspace_id, req.spec_id, req.mode, req.requested_by, req.idempotency_key, req_id, trace_id, branch, now_ms }) catch {
        common.internalOperationError(r, "Failed to create run", req_id);
        return;
    };
    defer insert.deinit();

    const inserted_row = insert.next() catch null orelse {
        common.internalOperationError(r, "Failed to upsert run", req_id);
        return;
    };
    const final_run_id = inserted_row.get([]u8, 0) catch run_id;
    const final_state = inserted_row.get([]u8, 1) catch "SPEC_QUEUED";
    const final_attempt = inserted_row.get(i32, 2) catch 1;
    const run_snapshot_version = inserted_row.get(?[]u8, 3) catch null;
    const tenant_id = inserted_row.get([]u8, 4) catch "";
    const was_inserted = inserted_row.get(bool, 5) catch false;

    if (!id_format.isSupportedRunId(final_run_id)) {
        common.errorResponse(r, .internal_server_error, error_codes.ERR_UUIDV7_CANONICAL_FORMAT, "Non-canonical run_id persisted", req_id);
        return;
    }

    if (was_inserted) {
        policy.recordPolicyEvent(conn, req.workspace_id, final_run_id, .sensitive, .allow, "m1.start_run", req.requested_by) catch |err| {
            obs_log.logWarnErr(.http, err, "policy event insert failed (non-fatal) run_id={s}", .{final_run_id});
        };

        log.info("run created run_id={s} workspace_id={s} spec_id={s}", .{
            final_run_id, req.workspace_id, req.spec_id,
        });
        if (run_snapshot_version) |snapshot| {
            profile_linkage.insertRunArtifact(conn, tenant_id, req.workspace_id, final_run_id, snapshot, now_ms) catch |err| {
                obs_log.logWarnErr(.http, err, "run linkage artifact persist failed run_id={s}", .{final_run_id});
                common.compensateStartRunQueueFailure(conn, final_run_id);
                common.internalOperationError(r, "Failed to persist run linkage artifact", req_id);
                return;
            };
        }
        ctx.queue.xaddRun(final_run_id, 0, req.workspace_id) catch |err| {
            obs_log.logWarnErr(.http, err, "queue enqueue failed run_id={s} workspace_id={s}", .{
                final_run_id,
                req.workspace_id,
            });
            common.compensateStartRunQueueFailure(conn, final_run_id);
            common.errorResponse(r, .service_unavailable, queue_unavailable_code, queue_unavailable_message, req_id);
            return;
        };
        metrics.incRunsCreated();
    } else {
        log.info("run idempotent replay run_id={s} workspace_id={s}", .{ final_run_id, req.workspace_id });
    }

    common.writeJson(r, .accepted, .{
        .run_id = final_run_id,
        .state = final_state,
        .attempt = @as(u32, @intCast(final_attempt)),
        .run_snapshot_version = run_snapshot_version,
        .request_id = req_id,
        .trace_id = trace_id,
    });
}
