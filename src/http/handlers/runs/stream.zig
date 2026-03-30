//! SSE stream handler for M18_001.
//! Subscribes to Redis pub/sub channel run:{id}:events and emits SSE events.
//! Sends a heartbeat comment every 30 seconds to prevent proxy timeouts.
//! Supports Last-Event-ID for reconnect replay from gate_results table.

const std = @import("std");
const httpz = @import("httpz");
const common = @import("../common.zig");
const obs_log = @import("../../../observability/logging.zig");
const queue_pubsub = @import("../../../queue/redis_pubsub.zig");
const id_format = @import("../../../types/id_format.zig");
const error_codes = @import("../../../errors/codes.zig");

const log = std.log.scoped(.http);

const TERMINAL_STATES = &[_][]const u8{ "DONE", "BLOCKED", "FAILED", "CANCELLED" };

fn isTerminalState(state: []const u8) bool {
    for (TERMINAL_STATES) |ts| {
        if (std.mem.eql(u8, state, ts)) return true;
    }
    return false;
}

pub fn handleStreamRun(ctx: *common.Context, req: *httpz.Request, res: *httpz.Response, run_id: []const u8) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch |err| {
        common.writeAuthError(res, req_id, err);
        return;
    };

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
    const initial_state = row.get([]u8, 2) catch "unknown";

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, workspace_id)) {
        common.errorResponse(res, .forbidden, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    run_result.drain() catch |err| obs_log.logWarnErr(.http, err, "stream.run_drain_fail run_id={s}", .{run_id});

    // Set SSE headers before any chunks
    res.header("Content-Type", "text/event-stream");
    res.header("Cache-Control", "no-cache");
    res.header("Connection", "keep-alive");
    res.header("X-Accel-Buffering", "no");

    // If already terminal, emit stored events and close immediately
    if (isTerminalState(initial_state)) {
        streamStoredEvents(alloc, conn, res, run_id, 0);
        const term_event = std.fmt.allocPrint(
            alloc,
            "event: run_complete\ndata: {{\"state\":\"{s}\"}}\n\n",
            .{initial_state},
        ) catch return;
        _ = res.chunk(term_event) catch {};
        return;
    }

    // Get Last-Event-ID for reconnect replay
    const last_event_id: i64 = blk: {
        const header = req.header("last-event-id") orelse break :blk 0;
        break :blk std.fmt.parseInt(i64, header, 10) catch 0;
    };

    // Replay missed events from DB if Last-Event-ID provided
    if (last_event_id > 0) {
        streamStoredEvents(alloc, conn, res, run_id, last_event_id);
    }

    // Connect to Redis pub/sub
    var subscriber = queue_pubsub.Subscriber.connectFromEnv(alloc) catch |err| {
        log.warn("stream.pubsub_connect_fail err={s} run_id={s}", .{ @errorName(err), run_id });
        streamViaPoll(alloc, conn, res, run_id);
        return;
    };
    defer subscriber.deinit();

    const channel = std.fmt.allocPrint(alloc, "run:{s}:events", .{run_id}) catch {
        res.status = 500;
        return;
    };

    subscriber.subscribe(channel) catch |err| {
        log.warn("stream.subscribe_fail err={s} run_id={s}", .{ @errorName(err), run_id });
        streamViaPoll(alloc, conn, res, run_id);
        return;
    };

    // Event loop: relay pub/sub messages as SSE events, heartbeat every 30s
    var event_seq: i64 = 0;
    const heartbeat_interval_ns: u64 = 30 * std.time.ns_per_s;
    var last_heartbeat = std.time.nanoTimestamp();

    while (true) {
        const now = std.time.nanoTimestamp();
        const elapsed: u64 = @intCast(@max(0, now - last_heartbeat));

        if (elapsed >= heartbeat_interval_ns) {
            res.chunk(": heartbeat\n\n") catch break;
            last_heartbeat = std.time.nanoTimestamp();
        }

        const msg_opt = subscriber.readMessage() catch break;

        if (msg_opt) |msg_val| {
            var msg = msg_val;
            defer msg.deinit();

            event_seq += 1;
            const event = std.fmt.allocPrint(
                alloc,
                "id: {d}\nevent: gate_result\ndata: {s}\n\n",
                .{ event_seq, msg.data },
            ) catch continue;

            res.chunk(event) catch break;
        }

        // Check if run reached terminal state
        var state_result = conn.query("SELECT state FROM runs WHERE run_id = $1", .{run_id}) catch continue;
        defer state_result.deinit();
        if (state_result.next() catch null) |srow| {
            const state = srow.get([]u8, 0) catch continue;
            if (isTerminalState(state)) {
                state_result.drain() catch {};
                const term_event = std.fmt.allocPrint(
                    alloc,
                    "event: run_complete\ndata: {{\"state\":\"{s}\"}}\n\n",
                    .{state},
                ) catch break;
                _ = res.chunk(term_event) catch {};
                break;
            }
        }
        state_result.drain() catch {};
    }
}

/// Emit SSE events for gate results already in the DB (replay / terminal runs).
/// `after_created_at` is 0 to emit all, or a timestamp for partial replay.
fn streamStoredEvents(
    alloc: std.mem.Allocator,
    conn: anytype,
    res: *httpz.Response,
    run_id: []const u8,
    after_created_at: i64,
) void {
    var q = conn.query(
        \\SELECT gate_name, attempt, exit_code, wall_ms, created_at
        \\FROM gate_results WHERE run_id = $1 AND created_at > $2
        \\ORDER BY attempt ASC, created_at ASC
    , .{ run_id, after_created_at }) catch return;
    defer q.deinit();

    while (q.next() catch null) |qrow| {
        const gate_name = qrow.get([]u8, 0) catch continue;
        const attempt = qrow.get(i32, 1) catch 0;
        const exit_code = qrow.get(i32, 2) catch -1;
        const wall_ms = qrow.get(i64, 3) catch 0;
        const created_at = qrow.get(i64, 4) catch 0;

        const outcome = if (exit_code == 0) "PASS" else "FAIL";
        const data = std.fmt.allocPrint(alloc,
            \\{{"gate_name":"{s}","outcome":"{s}","exit_code":{d},"loop":{d},"wall_ms":{d}}}
        , .{ gate_name, outcome, exit_code, attempt, wall_ms }) catch continue;

        const event = std.fmt.allocPrint(
            alloc,
            "id: {d}\nevent: gate_result\ndata: {s}\n\n",
            .{ created_at, data },
        ) catch continue;

        res.chunk(event) catch return;
    }
    q.drain() catch {};
}

/// Fallback: poll DB every 2s if Redis pub/sub fails.
fn streamViaPoll(
    alloc: std.mem.Allocator,
    conn: anytype,
    res: *httpz.Response,
    run_id: []const u8,
) void {
    var last_created_at: i64 = 0;
    var seq: i64 = 0;

    while (true) {
        std.Thread.sleep(2 * std.time.ns_per_s);

        var q = conn.query(
            \\SELECT gate_name, attempt, exit_code, wall_ms, created_at
            \\FROM gate_results WHERE run_id = $1 AND created_at > $2
            \\ORDER BY attempt ASC, created_at ASC
        , .{ run_id, last_created_at }) catch continue;
        defer q.deinit();

        while (q.next() catch null) |qrow| {
            const gate_name = qrow.get([]u8, 0) catch continue;
            const attempt = qrow.get(i32, 1) catch 0;
            const exit_code = qrow.get(i32, 2) catch -1;
            const wall_ms = qrow.get(i64, 3) catch 0;
            const created_at = qrow.get(i64, 4) catch 0;

            last_created_at = @max(last_created_at, created_at);
            seq += 1;

            const outcome = if (exit_code == 0) "PASS" else "FAIL";
            const data = std.fmt.allocPrint(alloc,
                \\{{"gate_name":"{s}","outcome":"{s}","exit_code":{d},"loop":{d},"wall_ms":{d}}}
            , .{ gate_name, outcome, exit_code, attempt, wall_ms }) catch continue;

            const event = std.fmt.allocPrint(
                alloc,
                "id: {d}\nevent: gate_result\ndata: {s}\n\n",
                .{ seq, data },
            ) catch continue;

            res.chunk(event) catch return;
        }
        q.drain() catch {};

        var state_q = conn.query("SELECT state FROM runs WHERE run_id = $1", .{run_id}) catch continue;
        defer state_q.deinit();
        if (state_q.next() catch null) |srow| {
            const state = srow.get([]u8, 0) catch continue;
            if (isTerminalState(state)) {
                state_q.drain() catch {};
                const term = std.fmt.allocPrint(
                    alloc,
                    "event: run_complete\ndata: {{\"state\":\"{s}\"}}\n\n",
                    .{state},
                ) catch break;
                _ = res.chunk(term) catch {};
                break;
            }
        }
        state_q.drain() catch {};
    }
}
