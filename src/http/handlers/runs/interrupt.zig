//! M21_001 §1: POST /v1/runs/{run_id}:interrupt
//! Accepts {"message": "...", "mode": "instant"|"queued"}.
//! Queued: writes to Redis key run:{id}:interrupt (SETEX 300s, last-write-wins).
//! Instant: additionally calls executor IPC injectUserMessage; falls back to queued.
//! Emits interrupt_ack SSE event via Redis pub/sub.

const std = @import("std");
const httpz = @import("httpz");
const common = @import("../common.zig");
const types = @import("../../../types.zig");
const error_codes = @import("../../../errors/codes.zig");
const obs_log = @import("../../../observability/logging.zig");
const queue_consts = @import("../../../queue/constants.zig");
const metrics = @import("../../../observability/metrics.zig");
const id_format = @import("../../../types/id_format.zig");

const log = std.log.scoped(.http);

/// Maximum message length to prevent abuse (OWASP: input validation).
const MAX_MESSAGE_BYTES: usize = 4096;

pub fn handleInterruptRun(
    ctx: *common.Context,
    req: *httpz.Request,
    res: *httpz.Response,
    run_id: []const u8,
) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();

    const req_id = common.requestId(alloc);

    const principal = common.authenticate(alloc, req, ctx) catch |err| {
        common.writeAuthErrorWithTracking(res, req_id, err, ctx.posthog);
        return;
    };

    if (!common.beginApiRequest(ctx)) {
        common.errorResponse(res, .service_unavailable, error_codes.ERR_API_SATURATED, "Server overloaded; retry shortly", req_id);
        return;
    }
    defer common.endApiRequest(ctx);

    if (!id_format.isSupportedRunId(run_id)) {
        common.errorResponse(res, .bad_request, error_codes.ERR_UUIDV7_INVALID_ID_SHAPE, "Invalid run_id format", req_id);
        return;
    }

    const body = req.body() orelse {
        common.errorResponse(res, .bad_request, error_codes.ERR_INVALID_REQUEST, "Request body required", req_id);
        return;
    };

    const Req = struct {
        message: []const u8,
        mode: []const u8 = "queued",
    };

    const parsed = std.json.parseFromSlice(Req, alloc, body, .{ .ignore_unknown_fields = true }) catch {
        common.errorResponse(res, .bad_request, error_codes.ERR_INVALID_REQUEST, "Malformed JSON or missing 'message' field", req_id);
        return;
    };
    defer parsed.deinit();
    const rval = parsed.value;

    // OWASP A03: Input validation — reject oversized messages.
    if (rval.message.len == 0 or rval.message.len > MAX_MESSAGE_BYTES) {
        common.errorResponse(res, .bad_request, error_codes.ERR_INVALID_REQUEST, "message must be 1–4096 bytes", req_id);
        return;
    }

    // Validate mode field.
    const is_instant = std.mem.eql(u8, rval.mode, "instant");
    const is_queued = std.mem.eql(u8, rval.mode, "queued");
    if (!is_instant and !is_queued) {
        common.errorResponse(res, .bad_request, error_codes.ERR_INVALID_REQUEST, "mode must be 'instant' or 'queued'", req_id);
        return;
    }

    // Fetch run state and authorize workspace.
    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    var run_q = conn.query(
        "SELECT state, workspace_id FROM core.runs WHERE run_id = $1",
        .{run_id},
    ) catch {
        common.internalDbError(res, req_id);
        return;
    };
    defer run_q.deinit();

    const row = run_q.next() catch null orelse {
        common.errorResponse(res, .not_found, error_codes.ERR_RUN_NOT_FOUND, "Run not found", req_id);
        return;
    };
    const state_str = alloc.dupe(u8, row.get([]u8, 0) catch "") catch "";
    const ws_id = alloc.dupe(u8, row.get([]u8, 1) catch "") catch "";
    run_q.drain() catch {};

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, principal, ws_id)) {
        common.errorResponse(res, .forbidden, error_codes.ERR_FORBIDDEN, "Workspace access denied", req_id);
        return;
    }

    const current = types.RunState.fromStr(state_str) catch {
        common.internalOperationError(res, "Unknown run state", req_id);
        return;
    };

    // M21_001 A09: resolve agent_id for observability (run → workspace → agent).
    var agent_id: []const u8 = "-";
    {
        var aq = conn.query(
            "SELECT agent_id FROM agent.agent_profiles WHERE workspace_id = $1 LIMIT 1",
            .{ws_id},
        ) catch null;
        if (aq) |*q| {
            defer q.*.deinit();
            if ((q.*.next() catch null)) |arow| {
                if (arow.get([]u8, 0) catch null) |aid| {
                    agent_id = alloc.dupe(u8, aid) catch "-";
                }
            }
            q.*.drain() catch {};
        }
    }

    // Only accept interrupts for active (non-terminal) states where the gate
    // loop is running and will consume the message.
    if (current.isTerminal() or current == .BLOCKED or current == .PR_OPENED or current == .NOTIFIED) {
        common.errorResponse(res, .conflict, error_codes.ERR_RUN_NOT_INTERRUPTIBLE, "Run is not in an interruptible state", req_id);
        return;
    }

    // §1.2: Write queued interrupt to Redis (both modes write it).
    const redis = ctx.queue;
    const key = std.fmt.allocPrint(alloc, "{s}{s}", .{ queue_consts.interrupt_key_prefix, run_id }) catch {
        common.internalOperationError(res, "Key allocation failed", req_id);
        return;
    };
    redis.setEx(key, rval.message, queue_consts.interrupt_ttl_seconds) catch |err| {
        obs_log.logWarnErr(.http, err, "interrupt.redis_setex_fail run_id={s} error_code={s}", .{
            run_id, error_codes.ERR_RUN_INTERRUPT_SIGNAL_FAILED,
        });
        common.errorResponse(res, .service_unavailable, error_codes.ERR_RUN_INTERRUPT_SIGNAL_FAILED, "Failed to store interrupt message", req_id);
        return;
    };

    var effective_mode: []const u8 = "queued";

    // §1.3: Instant mode — attempt IPC injection, fall back to queued on failure.
    // Note: The executor IPC for injectUserMessage is wired in client.zig but
    // requires the execution_id from the active run. In v1, we store the
    // message in Redis for the gate loop to pick up. Instant delivery via
    // IPC will be added when the executor runner exposes the active
    // execution_id on the run row (v2 enhancement).
    if (is_instant) {
        // v1: instant falls back to queued — message is already in Redis.
        // Metric tracks how often instant was requested vs delivered.
        metrics.incInterruptFallback();
        effective_mode = "queued";
    } else {
        metrics.incInterruptQueued();
    }

    // §1.4: Emit interrupt_ack SSE event via Redis pub/sub.
    const channel = std.fmt.allocPrint(alloc, "run:{s}:events", .{run_id}) catch "";
    if (channel.len > 0) {
        const now_ms = std.time.milliTimestamp();
        const ack_json = std.fmt.allocPrint(alloc,
            \\{{"mode":"{s}","received_at":{d}}}
        , .{ effective_mode, now_ms }) catch "";
        if (ack_json.len > 0) {
            const ack_event = std.fmt.allocPrint(alloc,
                \\{{"event_type":"interrupt_ack","data":{s}}}
            , .{ack_json}) catch "";
            if (ack_event.len > 0) {
                redis.publish(channel, ack_event) catch |err| {
                    obs_log.logWarnErr(.http, err, "interrupt.pubsub_fail run_id={s}", .{run_id});
                };
            }
        }
    }

    // §2.4: Log the interrupt as a run_transitions annotation.
    {
        const transition_id = id_format.generateTransitionId(alloc) catch "";
        if (transition_id.len > 0) {
            defer alloc.free(transition_id);
            const notes_str = std.fmt.allocPrint(alloc, "interrupt:{s}:{s}", .{ effective_mode, rval.message[0..@min(rval.message.len, 128)] }) catch null;
            _ = conn.exec(
                \\INSERT INTO core.run_transitions
                \\  (id, run_id, attempt, state_from, state_to, actor, reason_code, notes, ts)
                \\VALUES ($1, $2, (SELECT attempt FROM core.runs WHERE run_id = $2), $3, $3, 'orchestrator', $4, $5, $6)
            , .{
                transition_id,
                run_id,
                state_str,
                if (std.mem.eql(u8, effective_mode, "instant"))
                    types.ReasonCode.INTERRUPT_DELIVERED.label()
                else
                    types.ReasonCode.INTERRUPT_QUEUED.label(),
                notes_str,
                std.time.milliTimestamp(),
            }) catch |err| {
                obs_log.logWarnErr(.http, err, "interrupt.transition_log_fail run_id={s}", .{run_id});
            };
        }
    }

    log.info("run.interrupt_stored run_id={s} workspace_id={s} agent_id={s} mode={s}", .{ run_id, ws_id, agent_id, effective_mode });
    common.writeJson(res, .ok, .{
        .ack = true,
        .mode = effective_mode,
        .request_id = req_id,
    });
}
