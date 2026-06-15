//! GET /v1/workspaces/{ws}/agents/{id}/events/stream — Server-Sent
//! Events tail of the Redis pub/sub channel `agent:{id}:activity`.
//!
//! Connection lifecycle:
//!   1. Claim a StreamRegistry slot (cap or shutdown drain → 503) and
//!      authorize (Bearer middleware + path-workspace ownership).
//!   2. Subscribe to the channel through the process's SubscriptionHub —
//!      the hub owns the ONE shared Redis pub/sub connection; opening a
//!      stream costs a map entry, never a Redis dial or TLS handshake.
//!   3. Hand the TCP stream to a DEDICATED detached thread via
//!      `startEventStream` — never the pool-parking sync variant: a parked
//!      stream would pin a handler-pool thread for its whole lifetime (and
//!      pre-Patch-2 httpz round-robined private per-thread queues with no
//!      work-stealing, so a parked pool thread black-holed its queue's share
//!      of every later request — see vendor/httpz/CHANGES.md).
//!   4. Loop: timed-pop the subscription queue → write one SSE frame;
//!      timeout → heartbeat comment (probes client liveness); hub closed →
//!      exit (shutdown drain).
//!   5. On client disconnect (write error), hub close, or a registry drain
//!      (shutdown() of the client socket at process shutdown), the thread
//!      unsubscribes, releases its registry slot, and closes the socket —
//!      ownership of the fd is the thread's from startEventStream's disown
//!      onward, so the close here is what returns it to the OS.
//!
//! Hub-loss behaviour: a dead shared connection is invisible here — the
//! queue goes quiet, heartbeats keep the client alive, and the hub's
//! reconnect sweep resumes delivery. Frames published during the gap follow
//! the documented pub/sub loss semantics (clients backfill via the events
//! cursor).
//!
//! Auth (this slice):
//!   Bearer token via the `bearer()` middleware (CLI / programmatic
//!   path). The cookie auth path that the browser dashboard needs
//!   lands with slice 10 (UI), since the dashboard does not exist yet
//!   and the cookie session shape will be designed there.
//!
//! Sequence IDs are per-connection and reset to 0 on every new SUBSCRIBE.
//! Clients backfill via `GET /events?cursor=<last_event_id>` after a
//! reconnect; the new SSE then resumes from sequence 0. The server
//! ignores `Last-Event-ID` request headers.

const std = @import("std");
const clock = @import("common").clock;
const httpz = @import("httpz");
const pg = @import("pg");
const logging = @import("log");

const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const metrics = @import("../../../observability/metrics.zig");
const subscription_hub = @import("../../../events/subscription_hub.zig");

const log = logging.scoped(.http_agent_events_stream);

const Hx = hx_mod.Hx;

const channel_prefix = "agent:";
const channel_suffix = ":activity";

/// Idle wake-up cadence for the subscription pop. Each tick with no frames
/// sends a heartbeat comment so a vanished client is detected by the failing
/// write — without it a stream over a dead client would hold its thread and
/// slot until a publish that may never come.
const SSE_HEARTBEAT_INTERVAL_MS: u32 = 15_000;
/// Channel name scratch: prefix + UUID + suffix.
const CHANNEL_BUF_LEN: usize = 128;
/// SSE comment frame — ignored by EventSource clients, but the write probes
/// client liveness and keeps intermediaries from idling the connection out.
const SSE_HEARTBEAT_FRAME = ": heartbeat\n\n";

pub fn innerEventsStream(
    hx: Hx,
    req: *httpz.Request,
    workspace_id: []const u8,
    agent_id: []const u8,
) void {
    _ = req;
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    if (!id_format.isSupportedWorkspaceId(agent_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "agent_id must be a UUIDv7");
        return;
    }

    // Claim a registry slot before any backend work — shedding must stay
    // cheap under a tab-storm (one mutexed check-and-insert; bearer authn
    // already ran in the middleware chain). Null = at cap OR draining.
    const reg_id = (hx.ctx.stream_registry.tryRegister(workspace_id, agent_id, clock.nowMillis(), hx.ctx.sse_max_streams) catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    }) orelse {
        metrics.incSseBackpressureRejections();
        log.warn("stream_cap_rejected", .{
            .error_code = ec.ERR_SSE_STREAM_CAP,
            .live = hx.ctx.stream_registry.count(),
            .max = hx.ctx.sse_max_streams,
        });
        hx.res.header(common.HEADER_RETRY_AFTER, common.RETRY_AFTER_BRIEF_VALUE);
        hx.fail(ec.ERR_SSE_STREAM_CAP, ec.MSG_SSE_STREAM_CAP);
        return;
    };
    var handed_off = false;
    defer if (!handed_off) hx.ctx.stream_registry.deregister(reg_id);

    if (!authorize(hx, workspace_id, agent_id)) return;
    handed_off = startStreamThread(hx, agent_id, reg_id);
}

/// Returns true when stream ownership (job + slot) transferred to the
/// detached thread; false when a response was written on the request path.
fn startStreamThread(hx: Hx, agent_id: []const u8, reg_id: u64) bool {
    const job = StreamJob.create(hx.ctx, agent_id, reg_id) catch |err| {
        switch (err) {
            error.ChannelTooLong => common.internalDbError(hx.res, hx.req_id),
            // OOM or a hub already in shutdown — the stream surface is
            // momentarily unavailable, not the client's fault.
            error.OutOfMemory, error.HubStopped => common.internalDbUnavailable(hx.res, hx.req_id),
        }
        return false;
    };
    // startEventStream writes the SSE headers, flips the socket to blocking
    // mode, disowns the response, and runs streamThreadMain on a detached
    // thread — the handler-pool thread returns immediately (see the module
    // header for why a stream must never park a pool thread).
    hx.res.startEventStream(job, streamThreadMain) catch |err| {
        log.warn("sse_start_failed", .{ .err = @errorName(err) });
        job.destroy();
        return false;
    };
    return true;
}

fn streamThreadMain(job: *StreamJob, stream: std.Io.net.Stream) void {
    const ctx = job.ctx; // borrowed: boot-owned, outlives every stream thread
    const reg_id = job.reg_id;
    // LIFO teardown: destroy first (hub unsubscribe + job free), then the
    // registry slot (the test drain-polls' ordering guarantee: a freed slot
    // implies the job is gone), and the socket close LAST — an entry still
    // in the registry guarantees its fd is open, so a concurrent drain can
    // never shutdown() a reused descriptor. The close itself returns the
    // disowned fd to the OS (it leaked before the registry owned shutdown).
    defer stream.close(ctx.io);
    defer ctx.stream_registry.deregister(reg_id);
    defer job.destroy();
    ctx.stream_registry.attachFd(reg_id, stream.socket.handle);
    streamLoop(ctx.io, ctx.alloc, job.sub, stream) catch |err| {
        // Most "errors" here are client disconnects mid-write (broken pipe).
        // Log at debug — the operator-visible event is the connection close,
        // not the inner write error.
        log.debug("sse_stream_loop_exit", .{ .err = @errorName(err) });
    };
}

/// Everything the detached stream thread owns once the request returns: the
/// hub subscription handle. Allocated on ctx.alloc, NOT the request arena —
/// the arena dies when the handler returns, the thread does not. Single
/// owner: created on the request thread, destroyed by the stream thread (or
/// by startStreamThread when the spawn fails).
const StreamJob = struct {
    const Self = @This();

    ctx: *common.Context,
    sub: *subscription_hub.Subscription,
    reg_id: u64,

    const CreateError = error{ OutOfMemory, ChannelTooLong, HubStopped };

    fn create(ctx: *common.Context, agent_id: []const u8, reg_id: u64) CreateError!*StreamJob {
        const alloc = ctx.alloc;
        var channel_buf: [CHANNEL_BUF_LEN]u8 = undefined;
        const name = std.fmt.bufPrint(&channel_buf, "{s}{s}{s}", .{ channel_prefix, agent_id, channel_suffix }) catch
            return error.ChannelTooLong;
        const job = alloc.create(StreamJob) catch return error.OutOfMemory;
        errdefer alloc.destroy(job);
        const sub = ctx.hub.subscribe(name) catch |err| {
            log.warn("hub_subscribe_failed", .{ .channel = name, .err = @errorName(err) });
            return err;
        };
        job.* = .{ .ctx = ctx, .sub = sub, .reg_id = reg_id };
        return job;
    }

    fn destroy(self: *Self) void {
        const alloc = self.ctx.alloc;
        // unsubscribe consumes the handle: refcount drop, wire UNSUBSCRIBE
        // on the channel's last viewer, subscription freed.
        self.ctx.hub.unsubscribe(self.sub);
        alloc.destroy(self);
    }
};

fn streamLoop(
    io: std.Io,
    alloc: std.mem.Allocator,
    sub: *subscription_hub.Subscription,
    stream: std.Io.net.Stream,
) !void {
    var seq: u64 = 0;
    var w = stream.writer(io, &.{});
    while (true) {
        switch (sub.pop(SSE_HEARTBEAT_INTERVAL_MS)) {
            .message => |payload| {
                defer alloc.free(payload);
                const kind = extractKind(payload) orelse "message";
                try writeFrame(&w, seq, kind, payload);
                seq +%= 1;
            },
            // A heartbeat write to a vanished client fails and unwinds the
            // loop, releasing the thread + subscription.
            .timeout => try w.interface.writeAll(SSE_HEARTBEAT_FRAME),
            // Hub shutdown drain: exit promptly so stop() never waits on us.
            .closed => return,
        }
    }
}

/// Extract the `kind` field from the JSON payload so the SSE `event:`
/// line can carry it. Anchors on the leading `{"kind":"` prefix so an
/// embedded "\"kind\":\"" inside a string field cannot poison the
/// dispatch. Best-effort — falls back to `message` if the publisher's
/// shape changes.
fn extractKind(payload: []const u8) ?[]const u8 {
    const prefix = "{\"kind\":\"";
    if (payload.len < prefix.len) return null;
    if (!std.mem.startsWith(u8, payload, prefix)) return null;
    const close = std.mem.indexOfScalarPos(u8, payload, prefix.len, '"') orelse return null;
    return payload[prefix.len..close];
}

fn writeFrame(w: anytype, seq: u64, kind: []const u8, data_json: []const u8) !void {
    var seq_buf: [24]u8 = undefined;
    const seq_str = try std.fmt.bufPrint(&seq_buf, "{d}", .{seq});
    try w.interface.writeAll("id: ");
    try w.interface.writeAll(seq_str);
    try w.interface.writeAll("\nevent: ");
    try w.interface.writeAll(kind);
    try w.interface.writeAll("\ndata: ");
    try w.interface.writeAll(data_json);
    try w.interface.writeAll("\n\n");
}

fn authorize(hx: Hx, workspace_id: []const u8, agent_id: []const u8) bool {
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return false;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return false;
    }
    return verifyAgentInWorkspace(hx, conn, workspace_id, agent_id);
}

fn verifyAgentInWorkspace(hx: Hx, conn: *pg.Conn, path_workspace_id: []const u8, agent_id: []const u8) bool {
    var q = PgQuery.from(conn.query(
        "SELECT workspace_id::text FROM core.agents WHERE id = $1::uuid",
        .{agent_id},
    ) catch {
        common.internalDbError(hx.res, hx.req_id);
        return false;
    });
    defer q.deinit();
    const row = (q.next() catch {
        common.internalDbError(hx.res, hx.req_id);
        return false;
    }) orelse {
        hx.fail(ec.ERR_AGENTSFLEET_NOT_FOUND, ec.MSG_AGENTSFLEET_NOT_FOUND);
        return false;
    };
    const agent_workspace = row.get([]const u8, 0) catch {
        common.internalDbError(hx.res, hx.req_id);
        return false;
    };
    if (!std.mem.eql(u8, path_workspace_id, agent_workspace)) {
        hx.fail(ec.ERR_AGENTSFLEET_NOT_FOUND, ec.MSG_AGENTSFLEET_NOT_FOUND);
        return false;
    }
    return true;
}

// ── Tests ───────────────────────────────────────────────────────────────────

const testing = std.testing;

test "extractKind: parses leading kind field" {
    try testing.expectEqualStrings("event_received", extractKind("{\"kind\":\"event_received\",\"event_id\":\"x\"}").?);
    try testing.expectEqualStrings("chunk", extractKind("{\"kind\":\"chunk\",\"text\":\"hi\"}").?);
}

test "extractKind: returns null when field missing" {
    try testing.expect(extractKind("{\"foo\":\"bar\"}") == null);
}

test "extractKind: ignores embedded kind inside a string value" {
    // If a chunk's text happens to contain the kind-needle literal, the
    // anchored prefix scan must not pick it up — the real kind comes
    // first per the publisher's frame shape.
    const poisoned = "{\"kind\":\"chunk\",\"text\":\"\\\"kind\\\":\\\"fake\\\"\"}";
    try testing.expectEqualStrings("chunk", extractKind(poisoned).?);
}

test "extractKind: returns null when kind is not the leading field" {
    try testing.expect(extractKind("{\"event_id\":\"x\",\"kind\":\"chunk\"}") == null);
}

test "extractKind: handles short payloads without panicking" {
    try testing.expect(extractKind("") == null);
    try testing.expect(extractKind("{\"k\"") == null);
}
