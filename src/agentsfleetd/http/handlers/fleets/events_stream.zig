//! GET /v1/workspaces/{ws}/fleets/{id}/events/stream — Server-Sent
//! Events tail of the Redis pub/sub channel `fleet:{id}:activity`.
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
const activity_channel = @import("../../../events/activity_channel.zig");
const sse_frame = @import("../sse_frame.zig");

const log = logging.scoped(.http_fleet_events_stream);

const Hx = hx_mod.Hx;

pub fn innerEventsStream(
    hx: Hx,
    req: *httpz.Request,
    workspace_id: []const u8,
    fleet_id: []const u8,
) void {
    _ = req;
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    if (!id_format.isSupportedWorkspaceId(fleet_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "fleet_id must be a UUIDv7");
        return;
    }

    // Claim a registry slot before any backend work — shedding must stay
    // cheap under a tab-storm (one mutexed check-and-insert; bearer authn
    // already ran in the middleware chain). Null = at cap OR draining.
    const reg_id = (hx.ctx.stream_registry.tryRegister(workspace_id, fleet_id, clock.nowMillis(), hx.ctx.sse_max_streams) catch {
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

    if (!authorize(hx, workspace_id, fleet_id)) return;
    handed_off = startStreamThread(hx, fleet_id, reg_id);
}

/// Returns true when stream ownership (job + slot) transferred to the
/// detached thread; false when a response was written on the request path.
fn startStreamThread(hx: Hx, fleet_id: []const u8, reg_id: u64) bool {
    const job = StreamJob.create(hx.ctx, fleet_id, reg_id) catch |err| {
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
        log.warn("sse_start_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
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

    fn create(ctx: *common.Context, fleet_id: []const u8, reg_id: u64) CreateError!*StreamJob {
        const alloc = ctx.alloc;
        var channel_buf: [activity_channel.BUF_LEN]u8 = undefined;
        const name = try activity_channel.format(&channel_buf, fleet_id);
        const job = alloc.create(StreamJob) catch return error.OutOfMemory;
        errdefer alloc.destroy(job);
        const sub = ctx.hub.subscribe(name) catch |err| {
            log.warn("hub_subscribe_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .channel = name, .err = @errorName(err) });
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
        switch (sub.pop(sse_frame.HEARTBEAT_INTERVAL_MS)) {
            .message => |payload| {
                defer alloc.free(payload);
                const kind = sse_frame.extractKind(payload) orelse sse_frame.DEFAULT_KIND;
                try sse_frame.writeFrame(&w, seq, kind, payload);
                seq +%= 1;
            },
            // A heartbeat write to a vanished client fails and unwinds the
            // loop, releasing the thread + subscription.
            .timeout => try w.interface.writeAll(sse_frame.HEARTBEAT_FRAME),
            // Hub shutdown drain: exit promptly so stop() never waits on us.
            .closed => return,
        }
    }
}

fn authorize(hx: Hx, workspace_id: []const u8, fleet_id: []const u8) bool {
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return false;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return false;
    }
    return verifyFleetInWorkspace(hx, conn, workspace_id, fleet_id);
}

fn verifyFleetInWorkspace(hx: Hx, conn: *pg.Conn, path_workspace_id: []const u8, fleet_id: []const u8) bool {
    var q = PgQuery.from(conn.query(
        "SELECT workspace_id::text FROM core.fleets WHERE id = $1::uuid",
        .{fleet_id},
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
    const fleet_workspace = row.get([]const u8, 0) catch {
        common.internalDbError(hx.res, hx.req_id);
        return false;
    };
    if (!std.mem.eql(u8, path_workspace_id, fleet_workspace)) {
        hx.fail(ec.ERR_AGENTSFLEET_NOT_FOUND, ec.MSG_AGENTSFLEET_NOT_FOUND);
        return false;
    }
    return true;
}

// Frame-shape tests moved with the writers into `handlers/sse_frame.zig` —
// both SSE handlers share one tested copy (RULE NDC).
