const std = @import("std");
const zap = @import("zap");
const pg = @import("pg");
const state = @import("../../state/machine.zig");
const policy = @import("../../state/policy.zig");
const metrics = @import("../../observability/metrics.zig");
const obs_log = @import("../../observability/logging.zig");
const worker = @import("../../pipeline/worker.zig");
const common = @import("common.zig");
const log = std.log.scoped(.http);

const queue_unavailable_code = "QUEUE_UNAVAILABLE";
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
        common.errorResponse(r, .bad_request, "INVALID_REQUEST", "Request body required", req_id);
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
        common.errorResponse(r, .bad_request, "INVALID_REQUEST", "Malformed JSON or missing required fields", req_id);
        return;
    };
    defer parsed.deinit();
    const req = parsed.value;

    if (!common.beginApiRequest(ctx)) {
        common.errorResponse(r, .service_unavailable, "API_SATURATED", "Server overloaded; retry shortly", req_id);
        return;
    }
    defer common.endApiRequest(ctx);

    const conn = ctx.pool.acquire() catch {
        common.errorResponse(r, .service_unavailable, "INTERNAL_ERROR", "Database unavailable", req_id);
        return;
    };
    defer ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, principal, req.workspace_id)) {
        common.errorResponse(r, .forbidden, "FORBIDDEN", "Workspace access denied", req_id);
        return;
    }

    {
        var ws_check = conn.query(
            "SELECT paused FROM workspaces WHERE workspace_id = $1",
            .{req.workspace_id},
        ) catch {
            common.errorResponse(r, .internal_server_error, "INTERNAL_ERROR", "Database error", req_id);
            return;
        };
        defer ws_check.deinit();

        const ws_row = ws_check.next() catch null orelse {
            common.errorResponse(r, .not_found, "WORKSPACE_NOT_FOUND", "Workspace not found", req_id);
            return;
        };

        const paused = ws_row.get(bool, 0) catch false;
        if (paused) {
            common.errorResponse(r, .conflict, "WORKSPACE_PAUSED", "Workspace is paused", req_id);
            return;
        }
    }

    {
        var spec_check = conn.query(
            "SELECT spec_id FROM specs WHERE spec_id = $1 AND workspace_id = $2",
            .{ req.spec_id, req.workspace_id },
        ) catch {
            common.errorResponse(r, .internal_server_error, "INTERNAL_ERROR", "Database error", req_id);
            return;
        };
        defer spec_check.deinit();

        if (spec_check.next() catch null == null) {
            common.errorResponse(r, .not_found, "SPEC_NOT_FOUND", "Spec not found", req_id);
            return;
        }
    }

    var raw: [16]u8 = undefined;
    std.crypto.random.bytes(&raw);
    const run_id = std.fmt.allocPrint(alloc, "r_{s}", .{std.fmt.bytesToHex(raw, .lower)[0..16]}) catch "r_unknown";

    const now_ms = std.time.milliTimestamp();
    const branch = std.fmt.allocPrint(alloc, "zombie/run-{s}", .{run_id}) catch "zombie/run-unknown";

    var insert = conn.query(
        \\INSERT INTO runs
        \\  (run_id, workspace_id, spec_id, tenant_id, state, attempt, mode,
        \\   requested_by, idempotency_key, request_id, branch, created_at, updated_at)
        \\SELECT $1, $2, $3, tenant_id, 'SPEC_QUEUED', 1, $4, $5, $6, $7, $8, $9, $9
        \\FROM workspaces WHERE workspace_id = $2
        \\ON CONFLICT (workspace_id, idempotency_key) DO UPDATE
        \\SET updated_at = runs.updated_at
        \\RETURNING run_id, state, attempt, (xmax = 0) AS inserted
    , .{ run_id, req.workspace_id, req.spec_id, req.mode, req.requested_by, req.idempotency_key, req_id, branch, now_ms }) catch {
        common.errorResponse(r, .internal_server_error, "INTERNAL_ERROR", "Failed to create run", req_id);
        return;
    };
    defer insert.deinit();

    const inserted_row = insert.next() catch null orelse {
        common.errorResponse(r, .internal_server_error, "INTERNAL_ERROR", "Failed to upsert run", req_id);
        return;
    };
    const final_run_id = inserted_row.get([]u8, 0) catch run_id;
    const final_state = inserted_row.get([]u8, 1) catch "SPEC_QUEUED";
    const final_attempt = inserted_row.get(i32, 2) catch 1;
    const was_inserted = inserted_row.get(bool, 3) catch false;

    if (was_inserted) {
        policy.recordPolicyEvent(conn, req.workspace_id, final_run_id, .sensitive, .allow, "m1.start_run", req.requested_by) catch |err| {
            obs_log.logWarnErr(.http, err, "policy event insert failed (non-fatal) run_id={s}", .{final_run_id});
        };

        log.info("run created run_id={s} workspace_id={s} spec_id={s}", .{
            final_run_id, req.workspace_id, req.spec_id,
        });
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
        .request_id = req_id,
    });
}

pub fn handleGetRun(ctx: *common.Context, r: zap.Request, run_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, r, ctx) catch |err| {
        common.writeAuthError(r, req_id, err);
        return;
    };

    const conn = ctx.pool.acquire() catch {
        common.errorResponse(r, .service_unavailable, "INTERNAL_ERROR", "Database unavailable", req_id);
        return;
    };
    defer ctx.pool.release(conn);

    var run_result = conn.query(
        \\SELECT run_id, workspace_id, spec_id, state, attempt, mode,
        \\       requested_by, branch, pr_url, request_id, created_at, updated_at
        \\FROM runs WHERE run_id = $1
    , .{run_id}) catch {
        common.errorResponse(r, .internal_server_error, "INTERNAL_ERROR", "Database error", req_id);
        return;
    };
    defer run_result.deinit();

    const row = run_result.next() catch null orelse {
        common.errorResponse(r, .not_found, "RUN_NOT_FOUND", "Run not found", req_id);
        return;
    };

    const rid = row.get([]u8, 0) catch "?";
    const workspace_id = row.get([]u8, 1) catch "?";
    const spec_id = row.get([]u8, 2) catch "?";
    const run_state = row.get([]u8, 3) catch "?";
    const attempt = row.get(i32, 4) catch 1;
    const mode = row.get([]u8, 5) catch "api";
    const requested_by = row.get([]u8, 6) catch "?";
    const branch = row.get([]u8, 7) catch "?";
    const pr_url = row.get(?[]u8, 8) catch null;
    const run_request_id = row.get(?[]u8, 9) catch null;
    const created_at = row.get(i64, 10) catch 0;
    const updated_at = row.get(i64, 11) catch 0;

    if (!common.authorizeWorkspace(conn, principal, workspace_id)) {
        common.errorResponse(r, .forbidden, "FORBIDDEN", "Workspace access denied", req_id);
        return;
    }

    run_result.drain() catch |err| obs_log.logWarnErr(.http, err, "run query drain failed run_id={s}", .{run_id});

    var trans_result = conn.query(
        \\SELECT state_from, state_to, actor, reason_code, ts
        \\FROM run_transitions WHERE run_id = $1 ORDER BY ts ASC
    , .{run_id}) catch {
        common.errorResponse(r, .internal_server_error, "INTERNAL_ERROR", "Database error", req_id);
        return;
    };
    defer trans_result.deinit();

    var transitions: std.ArrayList(std.json.Value) = .empty;

    while (trans_result.next() catch null) |trow| {
        const tf = trow.get([]u8, 0) catch continue;
        const tt = trow.get([]u8, 1) catch continue;
        const ta = trow.get([]u8, 2) catch continue;
        const tc = trow.get([]u8, 3) catch continue;
        const ts = trow.get(i64, 4) catch 0;

        var obj = std.json.ObjectMap.init(alloc);
        obj.put("state_from", .{ .string = tf }) catch continue;
        obj.put("state_to", .{ .string = tt }) catch continue;
        obj.put("actor", .{ .string = ta }) catch continue;
        obj.put("reason_code", .{ .string = tc }) catch continue;
        obj.put("ts", .{ .integer = ts }) catch continue;
        transitions.append(alloc, .{ .object = obj }) catch continue;
    }
    trans_result.drain() catch |err| obs_log.logWarnErr(.http, err, "transitions query drain failed run_id={s}", .{run_id});

    var artifacts_arr: std.ArrayList(std.json.Value) = .empty;
    fetch_artifacts: {
        var art_result = conn.query(
            \\SELECT artifact_name, object_key, checksum_sha256, producer, attempt, created_at
            \\FROM artifacts WHERE run_id = $1 ORDER BY created_at ASC
        , .{run_id}) catch break :fetch_artifacts;
        defer art_result.deinit();
        while (art_result.next() catch null) |arow| {
            const aname = arow.get([]u8, 0) catch continue;
            const akey = arow.get([]u8, 1) catch continue;
            const achk = arow.get([]u8, 2) catch continue;
            const aprod = arow.get([]u8, 3) catch continue;
            const aattempt = arow.get(i32, 4) catch 1;
            const ats = arow.get(i64, 5) catch 0;

            var obj = std.json.ObjectMap.init(alloc);
            obj.put("artifact_name", .{ .string = aname }) catch continue;
            obj.put("object_key", .{ .string = akey }) catch continue;
            obj.put("checksum_sha256", .{ .string = achk }) catch continue;
            obj.put("producer", .{ .string = aprod }) catch continue;
            obj.put("attempt", .{ .integer = @as(i64, aattempt) }) catch continue;
            obj.put("created_at", .{ .integer = ats }) catch continue;
            artifacts_arr.append(alloc, .{ .object = obj }) catch continue;
        }
    }

    var policy_events_arr: std.ArrayList(std.json.Value) = .empty;
    fetch_policy_events: {
        var pe_result = conn.query(
            \\SELECT action_class, decision, rule_id, actor, ts
            \\FROM policy_events WHERE run_id = $1 ORDER BY ts ASC
        , .{run_id}) catch break :fetch_policy_events;
        defer pe_result.deinit();
        while (pe_result.next() catch null) |prow| {
            const pclass = prow.get([]u8, 0) catch continue;
            const pdec = prow.get([]u8, 1) catch continue;
            const prule = prow.get([]u8, 2) catch continue;
            const pactor = prow.get([]u8, 3) catch continue;
            const pts = prow.get(i64, 4) catch 0;

            var obj = std.json.ObjectMap.init(alloc);
            obj.put("action_class", .{ .string = pclass }) catch continue;
            obj.put("decision", .{ .string = pdec }) catch continue;
            obj.put("rule_id", .{ .string = prule }) catch continue;
            obj.put("actor", .{ .string = pactor }) catch continue;
            obj.put("ts", .{ .integer = pts }) catch continue;
            policy_events_arr.append(alloc, .{ .object = obj }) catch continue;
        }
    }

    common.writeJson(r, .ok, .{
        .run_id = rid,
        .workspace_id = workspace_id,
        .spec_id = spec_id,
        .current_state = run_state,
        .attempt = attempt,
        .mode = mode,
        .requested_by = requested_by,
        .branch = branch,
        .pr_url = pr_url,
        .run_request_id = run_request_id,
        .created_at = created_at,
        .updated_at = updated_at,
        .transitions = transitions.items,
        .artifacts = artifacts_arr.items,
        .policy_events = policy_events_arr.items,
        .request_id = req_id,
    });
}

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

    if (workspace_id_for_policy.len > 0 and !common.authorizeWorkspace(conn, principal, workspace_id_for_policy)) {
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

test "integration: beginApiRequest enforces max in-flight limit" {
    var ws = worker.WorkerState.init();
    var ctx = common.Context{
        .pool = undefined,
        .queue = undefined,
        .alloc = std.testing.allocator,
        .api_keys = "",
        .clerk = null,
        .worker_state = &ws,
        .api_in_flight_requests = std.atomic.Value(u32).init(0),
        .api_max_in_flight_requests = 2,
        .ready_max_queue_depth = null,
        .ready_max_queue_age_ms = null,
    };

    try std.testing.expect(common.beginApiRequest(&ctx));
    try std.testing.expect(common.beginApiRequest(&ctx));
    try std.testing.expect(!common.beginApiRequest(&ctx));
    try std.testing.expectEqual(@as(u32, 2), ctx.api_in_flight_requests.load(.acquire));
}

test "integration: endApiRequest decrements in-flight counter deterministically" {
    var ws = worker.WorkerState.init();
    var ctx = common.Context{
        .pool = undefined,
        .queue = undefined,
        .alloc = std.testing.allocator,
        .api_keys = "",
        .clerk = null,
        .worker_state = &ws,
        .api_in_flight_requests = std.atomic.Value(u32).init(0),
        .api_max_in_flight_requests = 1,
        .ready_max_queue_depth = null,
        .ready_max_queue_age_ms = null,
    };

    try std.testing.expect(common.beginApiRequest(&ctx));
    common.endApiRequest(&ctx);
    try std.testing.expectEqual(@as(u32, 0), ctx.api_in_flight_requests.load(.acquire));
}

test "integration: start-run queue failure compensation removes only SPEC_QUEUED row" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE runs (
            \\  run_id TEXT PRIMARY KEY,
            \\  state TEXT NOT NULL,
            \\  updated_at BIGINT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }

    const now_ms = std.time.milliTimestamp();
    {
        var q = try db_ctx.conn.query(
            "INSERT INTO runs (run_id, state, updated_at) VALUES ($1, 'SPEC_QUEUED', $2)",
            .{ "run-delete", now_ms },
        );
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            "INSERT INTO runs (run_id, state, updated_at) VALUES ($1, 'RUN_PLANNED', $2)",
            .{ "run-keep", now_ms },
        );
        q.deinit();
    }

    common.compensateStartRunQueueFailure(db_ctx.conn, "run-delete");
    common.compensateStartRunQueueFailure(db_ctx.conn, "run-keep");

    {
        var q = try db_ctx.conn.query("SELECT COUNT(*)::BIGINT FROM runs WHERE run_id = 'run-delete'", .{});
        defer q.deinit();
        const row = (q.next() catch null) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqual(@as(i64, 0), row.get(i64, 0) catch -1);
    }
    {
        var q = try db_ctx.conn.query("SELECT COUNT(*)::BIGINT FROM runs WHERE run_id = 'run-keep'", .{});
        defer q.deinit();
        const row = (q.next() catch null) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqual(@as(i64, 1), row.get(i64, 0) catch -1);
    }
}

test "integration: retry queue failure compensation restores state and removes retry transition" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.release(db_ctx.conn);
    defer db_ctx.pool.deinit();

    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE runs (
            \\  run_id TEXT PRIMARY KEY,
            \\  state TEXT NOT NULL,
            \\  updated_at BIGINT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE run_transitions (
            \\  run_id TEXT NOT NULL,
            \\  reason_code TEXT NOT NULL,
            \\  ts BIGINT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }

    const now_ms = std.time.milliTimestamp();
    const transition_ts = now_ms + 1;
    {
        var q = try db_ctx.conn.query(
            "INSERT INTO runs (run_id, state, updated_at) VALUES ($1, 'SPEC_QUEUED', $2)",
            .{ "run-retry", now_ms },
        );
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            "INSERT INTO run_transitions (run_id, reason_code, ts) VALUES ($1, 'MANUAL_RETRY', $2)",
            .{ "run-retry", transition_ts },
        );
        q.deinit();
    }

    common.compensateRetryQueueFailure(db_ctx.conn, "run-retry", "RUN_FAILED", transition_ts);

    {
        var q = try db_ctx.conn.query("SELECT state FROM runs WHERE run_id = $1", .{"run-retry"});
        defer q.deinit();
        const row = (q.next() catch null) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqualStrings("RUN_FAILED", row.get([]const u8, 0) catch "");
    }
    {
        var q = try db_ctx.conn.query(
            "SELECT COUNT(*)::BIGINT FROM run_transitions WHERE run_id = $1 AND reason_code = 'MANUAL_RETRY' AND ts = $2",
            .{ "run-retry", transition_ts },
        );
        defer q.deinit();
        const row = (q.next() catch null) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqual(@as(i64, 0), row.get(i64, 0) catch -1);
    }
}
