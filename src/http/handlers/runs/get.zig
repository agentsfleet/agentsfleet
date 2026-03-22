const std = @import("std");
const zap = @import("zap");
const common = @import("../common.zig");
const obs_log = @import("../../../observability/logging.zig");
const profile_linkage = @import("../../../audit/profile_linkage.zig");
const id_format = @import("../../../types/id_format.zig");
const error_codes = @import("../../../errors/codes.zig");

const log = std.log.scoped(.http);

const RunResponse = struct {
    run_id: []const u8,
    workspace_id: []const u8,
    spec_id: []const u8,
    current_state: []const u8,
    attempt: i32,
    mode: []const u8,
    requested_by: []const u8,
    branch: []const u8,
    pr_url: ?[]const u8,
    run_request_id: ?[]const u8,
    run_snapshot_version: ?[]const u8,
    created_at: i64,
    updated_at: i64,
    transitions: []const std.json.Value,
    artifacts: []const std.json.Value,
    policy_events: []const std.json.Value,
    profile_linkage: ?profile_linkage.RunLinkage,
    request_id: []const u8,
};

fn buildRunResponse(
    run_id: []const u8,
    workspace_id: []const u8,
    spec_id: []const u8,
    run_state: []const u8,
    attempt: i32,
    mode: []const u8,
    requested_by: []const u8,
    branch: []const u8,
    pr_url: ?[]const u8,
    run_request_id: ?[]const u8,
    run_snapshot_version: ?[]const u8,
    created_at: i64,
    updated_at: i64,
    transitions: []const std.json.Value,
    artifacts: []const std.json.Value,
    policy_events: []const std.json.Value,
    linkage: ?profile_linkage.RunLinkage,
    req_id: []const u8,
) RunResponse {
    return .{
        .run_id = run_id,
        .workspace_id = workspace_id,
        .spec_id = spec_id,
        .current_state = run_state,
        .attempt = attempt,
        .mode = mode,
        .requested_by = requested_by,
        .branch = branch,
        .pr_url = pr_url,
        .run_request_id = run_request_id,
        .run_snapshot_version = run_snapshot_version,
        .created_at = created_at,
        .updated_at = updated_at,
        .transitions = transitions,
        .artifacts = artifacts,
        .policy_events = policy_events,
        .profile_linkage = linkage,
        .request_id = req_id,
    };
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

    log.debug("run.get run_id={s}", .{run_id});

    if (!id_format.isSupportedRunId(run_id)) {
        common.errorResponse(r, .bad_request, error_codes.ERR_UUIDV7_INVALID_ID_SHAPE, "Invalid run_id format", req_id);
        return;
    }

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(r, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    var run_result = conn.query(
        \\SELECT run_id, workspace_id, spec_id, state, attempt, mode,
        \\       requested_by, branch, pr_url, request_id, run_snapshot_version, created_at, updated_at
        \\FROM runs WHERE run_id = $1
    , .{run_id}) catch {
        common.internalDbError(r, req_id);
        return;
    };
    defer run_result.deinit();

    const row = run_result.next() catch null orelse {
        common.errorResponse(r, .not_found, error_codes.ERR_RUN_NOT_FOUND, "Run not found", req_id);
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
    const run_snapshot_version = row.get(?[]u8, 10) catch null;
    const created_at = row.get(i64, 11) catch 0;
    const updated_at = row.get(i64, 12) catch 0;

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id)) {
        common.errorResponse(r, .forbidden, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    run_result.drain() catch |err| obs_log.logWarnErr(.http, err, "run.query_drain_fail run_id={s}", .{run_id});

    var trans_result = conn.query(
        \\SELECT state_from, state_to, actor, reason_code, ts
        \\FROM run_transitions WHERE run_id = $1 ORDER BY ts ASC
    , .{run_id}) catch {
        common.internalDbError(r, req_id);
        return;
    };
    defer trans_result.deinit();

    var transitions: std.ArrayList(std.json.Value) = .{};

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
    trans_result.drain() catch |err| obs_log.logWarnErr(.http, err, "run.transitions_drain_fail run_id={s}", .{run_id});

    var artifacts_arr: std.ArrayList(std.json.Value) = .{};
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

    var policy_events_arr: std.ArrayList(std.json.Value) = .{};
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

    var linkage: ?profile_linkage.RunLinkage = profile_linkage.fetchRunLinkage(conn, alloc, run_id) catch null;
    defer if (linkage) |*value| profile_linkage.freeRunLinkage(alloc, value);

    common.writeJson(r, .ok, buildRunResponse(
        rid,
        workspace_id,
        spec_id,
        run_state,
        attempt,
        mode,
        requested_by,
        branch,
        pr_url,
        run_request_id,
        run_snapshot_version,
        created_at,
        updated_at,
        transitions.items,
        artifacts_arr.items,
        policy_events_arr.items,
        linkage,
        req_id,
    ));
}

test "integration: get-run response payload includes profile_linkage chain contract" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE config_linkage_audit_artifacts (
            \\  artifact_id TEXT PRIMARY KEY,
            \\  tenant_id TEXT NOT NULL,
            \\  workspace_id TEXT NOT NULL,
            \\  artifact_type TEXT NOT NULL,
            \\  config_version_id TEXT NOT NULL,
            \\  compile_job_id TEXT,
            \\  run_id TEXT,
            \\  parent_artifact_id TEXT,
            \\  metadata_json TEXT NOT NULL DEFAULT '{}',
            \\  created_at BIGINT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }

    try profile_linkage.insertCompileArtifact(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", "0195b4ba-8d3a-7f13-9abc-2b3e1e0a6f98", "0195b4ba-8d3a-7f13-aabc-2b3e1e0a6f97", true, 10);
    try profile_linkage.insertActivateArtifact(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", "0195b4ba-8d3a-7f13-9abc-2b3e1e0a6f98", "operator", 20);
    try profile_linkage.insertRunArtifact(db_ctx.conn, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f99", "0195b4ba-8d3a-7f13-9abc-2b3e1e0a6f98", 30);

    var linkage = (try profile_linkage.fetchRunLinkage(db_ctx.conn, std.testing.allocator, "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f99")).?;
    defer profile_linkage.freeRunLinkage(std.testing.allocator, &linkage);

    const empty_values = [_]std.json.Value{};
    const payload = buildRunResponse(
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f99",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11",
        "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f55",
        "SPEC_QUEUED",
        1,
        "api",
        "operator",
        "zombie/run-0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f99",
        null,
        null,
        "0195b4ba-8d3a-7f13-9abc-2b3e1e0a6f98",
        30,
        30,
        empty_values[0..],
        empty_values[0..],
        empty_values[0..],
        linkage,
        "req_1",
    );
    const encoded = try std.json.Stringify.valueAlloc(std.testing.allocator, payload, .{});
    defer std.testing.allocator.free(encoded);

    const parsed = try std.json.parseFromSlice(std.json.Value, std.testing.allocator, encoded, .{});
    defer parsed.deinit();
    const obj = parsed.value.object;
    const linkage_value = obj.get("profile_linkage") orelse return error.TestUnexpectedResult;
    try std.testing.expectEqual(std.json.Value.Tag.object, linkage_value);
    const linkage_obj = linkage_value.object;

    const run_artifact_id = linkage_obj.get("run_artifact_id") orelse return error.TestUnexpectedResult;
    const activate_artifact_id = linkage_obj.get("activate_artifact_id") orelse return error.TestUnexpectedResult;
    const compile_artifact_id = linkage_obj.get("compile_artifact_id") orelse return error.TestUnexpectedResult;
    const config_version_id = linkage_obj.get("config_version_id") orelse return error.TestUnexpectedResult;
    const compile_job_id = linkage_obj.get("compile_job_id") orelse return error.TestUnexpectedResult;

    try std.testing.expectEqual(std.json.Value.Tag.string, run_artifact_id);
    try std.testing.expectEqual(std.json.Value.Tag.string, activate_artifact_id);
    try std.testing.expectEqual(std.json.Value.Tag.string, compile_artifact_id);
    try std.testing.expectEqual(std.json.Value.Tag.string, config_version_id);
    try std.testing.expectEqual(std.json.Value.Tag.string, compile_job_id);
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-9abc-2b3e1e0a6f98", config_version_id.string);
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-aabc-2b3e1e0a6f97", compile_job_id.string);
}
