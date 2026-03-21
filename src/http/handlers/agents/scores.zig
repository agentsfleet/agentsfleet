const std = @import("std");
const zap = @import("zap");
const common = @import("../common.zig");
const obs_log = @import("../../../observability/logging.zig");
const id_format = @import("../../../types/id_format.zig");
const error_codes = @import("../../../errors/codes.zig");

const log = std.log.scoped(.http);

const default_limit: i64 = 50;
const max_limit: i64 = 100;

const sql_resolve_agent_workspace =
    \\SELECT workspace_id FROM agent_profiles WHERE agent_id = $1
;

// score_id is UUIDv7: lexicographic order == chronological order, so
// `score_id < $cursor` gives stable keyset pagination without a subquery.
const sql_scores_first_page =
    \\SELECT score_id, run_id, score, axis_scores, weight_snapshot, scored_at
    \\FROM agent_run_scores
    \\WHERE agent_id = $1 AND workspace_id = $2
    \\ORDER BY score_id DESC
    \\LIMIT $3
;

const sql_scores_after_cursor =
    \\SELECT score_id, run_id, score, axis_scores, weight_snapshot, scored_at
    \\FROM agent_run_scores
    \\WHERE agent_id = $1 AND workspace_id = $2 AND score_id < $3
    \\ORDER BY score_id DESC
    \\LIMIT $4
;

pub fn handleGetAgentScores(ctx: *common.Context, r: zap.Request, agent_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, r, ctx) catch |err| {
        common.writeAuthError(r, req_id, err);
        return;
    };

    if (!id_format.isSupportedAgentId(agent_id)) {
        common.errorResponse(r, .bad_request, error_codes.ERR_UUIDV7_INVALID_ID_SHAPE, "Invalid agent_id format", req_id);
        return;
    }

    const limit: i64 = blk: {
        const param = r.getParamStr(alloc, "limit") catch null orelse break :blk default_limit;
        const parsed = std.fmt.parseInt(i64, param, 10) catch default_limit;
        break :blk @min(@max(parsed, 1), max_limit);
    };

    // Stripe-style cursor: score_id of the last item on the previous page.
    const starting_after: ?[]const u8 = r.getParamStr(alloc, "starting_after") catch null;

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(r, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    var wq = conn.query(sql_resolve_agent_workspace, .{agent_id}) catch {
        common.internalDbError(r, req_id);
        return;
    };
    defer wq.deinit();

    const wrow = wq.next() catch null orelse {
        common.errorResponse(r, .not_found, error_codes.ERR_AGENT_NOT_FOUND, "Agent not found", req_id);
        return;
    };
    const workspace_id = wrow.get([]u8, 0) catch {
        common.internalDbError(r, req_id);
        return;
    };
    wq.drain() catch |err| obs_log.logWarnErr(.http, err, "agent workspace drain failed agent_id={s}", .{agent_id});

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id)) {
        common.errorResponse(r, .forbidden, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    // Fetch limit+1 to detect whether a next page exists.
    const fetch_limit = limit + 1;

    var sq = if (starting_after) |cursor|
        conn.query(sql_scores_after_cursor, .{ agent_id, workspace_id, cursor, fetch_limit }) catch {
            common.internalDbError(r, req_id);
            return;
        }
    else
        conn.query(sql_scores_first_page, .{ agent_id, workspace_id, fetch_limit }) catch {
            common.internalDbError(r, req_id);
            return;
        };
    defer sq.deinit();

    var data: std.ArrayList(std.json.Value) = .{};
    while (sq.next() catch null) |srow| {
        const sid = srow.get([]u8, 0) catch continue;
        const run_id = srow.get([]u8, 1) catch continue;
        const score_i32 = srow.get(i32, 2) catch continue;
        const axis_scores = srow.get([]u8, 3) catch continue;
        const weight_snapshot = srow.get([]u8, 4) catch continue;
        const scored_at = srow.get(i64, 5) catch continue;

        var obj = std.json.ObjectMap.init(alloc);
        obj.put("score_id", .{ .string = sid }) catch continue;
        obj.put("run_id", .{ .string = run_id }) catch continue;
        obj.put("score", .{ .integer = @as(i64, score_i32) }) catch continue;
        obj.put("axis_scores", .{ .string = axis_scores }) catch continue;
        obj.put("weight_snapshot", .{ .string = weight_snapshot }) catch continue;
        obj.put("scored_at", .{ .integer = scored_at }) catch continue;
        data.append(alloc, .{ .object = obj }) catch continue;
    }
    sq.drain() catch |err| obs_log.logWarnErr(.http, err, "scores query drain failed agent_id={s}", .{agent_id});

    const result_count = @min(data.items.len, @as(usize, @intCast(limit)));
    const has_more = data.items.len > result_count;
    const result_data = data.items[0..result_count];

    const next_cursor: ?[]const u8 = if (has_more and result_count > 0) blk: {
        const last = result_data[result_count - 1];
        if (last.object.get("score_id")) |v| break :blk v.string;
        break :blk null;
    } else null;

    log.debug("scores retrieved agent_id={s} count={d} has_more={}", .{ agent_id, result_count, has_more });

    common.writeJson(r, .ok, .{
        .data = result_data,
        .has_more = has_more,
        .next_cursor = next_cursor,
        .request_id = req_id,
    });
}

test "integration: agent scores returns empty for unknown agent" {
    const db_ctx = (try common.openHandlerTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    var q = try db_ctx.conn.query(sql_resolve_agent_workspace, .{"0195b4ba-8d3a-7f13-8abc-000000000000"});
    defer q.deinit();
    try std.testing.expect(try q.next() == null);
}
