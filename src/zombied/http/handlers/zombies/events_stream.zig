//! GET /v1/workspaces/{ws}/zombies/{id}/events/stream — Server-Sent
//! Events tail of the Redis pub/sub channel `zombie:{id}:activity`.
//!
//! Connection lifecycle:
//!   1. Claim a stream slot (cap → 503) and authorize (Bearer middleware +
//!      path-workspace ownership).
//!   2. Issue `SUBSCRIBE zombie:{id}:activity` on a dedicated Redis
//!      connection — pub/sub blocks the conn, so we can NOT share the
//!      request-handler queue client.
//!   3. Hand the TCP stream to a DEDICATED detached thread via
//!      `startEventStream` — never the pool-parking sync variant: httpz's
//!      handler pool round-robins private per-thread queues with no
//!      work-stealing, so one pool thread parked on a stream black-holes its
//!      queue's share of every later request.
//!   4. Loop: read pub/sub message → write one SSE frame.
//!   5. On client disconnect (write error) or any read error, the thread
//!      frees its job and releases the slot.
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
const redis_subscriber = @import("../../../queue/redis_subscriber.zig");

const log = logging.scoped(.http_zombie_events_stream);

const Hx = hx_mod.Hx;

const channel_prefix = "zombie:";
const channel_suffix = ":activity";

/// Idle wake-up cadence for the SSE subscriber. Each tick with no pub/sub
/// traffic sends a heartbeat comment so a vanished client is detected by the
/// failing write — without it the stream thread parks on the Redis read
/// forever, holding its thread + a Redis connection until a publish that may
/// never come (dead client + idle zombie = a leaked stream slot).
const SSE_HEARTBEAT_INTERVAL_MS: u32 = 15_000;
/// Channel name scratch carried inside StreamJob: prefix + UUID + suffix.
const CHANNEL_BUF_LEN: usize = 128;
/// A `nextMessage` null returning in under half the heartbeat window is a
/// closed/RST socket, not an elapsed read timeout → exit instead of busy-
/// looping heartbeats against a dead Redis.
const SSE_TIMEOUT_MIN_ELAPSED_MS: i64 = SSE_HEARTBEAT_INTERVAL_MS / 2;
/// SSE comment frame — ignored by EventSource clients, but the write probes
/// client liveness and keeps intermediaries from idling the connection out.
const SSE_HEARTBEAT_FRAME = ": heartbeat\n\n";

const IdleAction = enum { heartbeat, close };

/// Read a null `nextMessage` from how long the read blocked: a full idle window
/// means SO_RCVTIMEO elapsed (heartbeat the client); a near-instant null means
/// the socket closed or reset (exit the loop).
fn classifyIdle(elapsed_ms: i64) IdleAction {
    return if (elapsed_ms < SSE_TIMEOUT_MIN_ELAPSED_MS) .close else .heartbeat;
}

pub fn innerEventsStream(
    hx: Hx,
    req: *httpz.Request,
    workspace_id: []const u8,
    zombie_id: []const u8,
) void {
    _ = req;
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    if (!id_format.isSupportedWorkspaceId(zombie_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "zombie_id must be a UUIDv7");
        return;
    }

    // Claim a stream slot before any backend work — shedding must stay cheap
    // under a tab-storm. Bearer authn already ran in the middleware chain.
    // safe because: pure admission counter — no memory is published through
    // it; the paired release runs in the defer below, or on the detached
    // stream thread once ownership hands off.
    const live = hx.ctx.sse_in_flight_streams.fetchAdd(1, .monotonic) + 1;
    var handed_off = false;
    defer if (!handed_off) releaseStreamSlot(hx.ctx);
    metrics.setSseInFlightStreams(live);
    if (live > hx.ctx.sse_max_streams) {
        metrics.incSseBackpressureRejections();
        log.warn("stream_cap_rejected", .{
            .error_code = ec.ERR_SSE_STREAM_CAP,
            .live = live,
            .max = hx.ctx.sse_max_streams,
        });
        hx.fail(ec.ERR_SSE_STREAM_CAP, ec.MSG_SSE_STREAM_CAP);
        return;
    }

    if (!authorize(hx, workspace_id, zombie_id)) return;
    handed_off = startStreamThread(hx, zombie_id);
}

/// Paired with the claim in innerEventsStream. Runs on the request thread
/// for rejected/failed streams, on the detached stream thread otherwise.
fn releaseStreamSlot(ctx: *common.Context) void {
    // safe because: same admission counter as the claim; the gauge store
    // tolerates last-writer staleness between concurrent streams.
    const after = ctx.sse_in_flight_streams.fetchSub(1, .monotonic) - 1;
    metrics.setSseInFlightStreams(after);
}

/// Returns true when stream ownership (job + slot) transferred to the
/// detached thread; false when a response was written on the request path.
fn startStreamThread(hx: Hx, zombie_id: []const u8) bool {
    const job = StreamJob.create(hx.ctx, zombie_id) catch |err| {
        switch (err) {
            error.ChannelTooLong => common.internalDbError(hx.res, hx.req_id),
            else => common.internalDbUnavailable(hx.res, hx.req_id),
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
    // LIFO defers: destroy runs first, the slot release last — an observer of
    // the freed slot (test drain-polls) has also observed the job teardown.
    defer releaseStreamSlot(ctx);
    defer job.destroy();
    streamLoop(ctx.io, ctx.alloc, &job.subscriber, stream) catch |err| {
        // Most "errors" here are client disconnects mid-write (broken pipe).
        // Log at debug — the operator-visible event is the connection close,
        // not the inner write error.
        log.debug("sse_stream_loop_exit", .{ .err = @errorName(err) });
    };
    job.subscriber.unsubscribe(job.channel());
}

/// Everything the detached stream thread owns once the request returns: the
/// dedicated pub/sub connection and the channel name. Allocated on ctx.alloc,
/// NOT the request arena — the arena dies when the handler returns, the
/// thread does not. Single owner: created on the request thread, destroyed by
/// the stream thread (or by startStreamThread when the spawn fails).
const StreamJob = struct {
    ctx: *common.Context,
    subscriber: redis_subscriber,
    channel_buf: [CHANNEL_BUF_LEN]u8,
    channel_len: usize,

    const CreateError = error{ OutOfMemory, ChannelTooLong, SubscriberConnectFailed, SubscribeFailed };

    fn create(ctx: *common.Context, zombie_id: []const u8) CreateError!*StreamJob {
        const alloc = ctx.alloc;
        const job = alloc.create(StreamJob) catch return error.OutOfMemory;
        errdefer alloc.destroy(job);
        job.ctx = ctx;
        const name = std.fmt.bufPrint(&job.channel_buf, "{s}{s}{s}", .{ channel_prefix, zombie_id, channel_suffix }) catch
            return error.ChannelTooLong;
        job.channel_len = name.len;
        job.subscriber = redis_subscriber.connectFromConfig(ctx.io, alloc, ctx.queue.pool.cfg, .{ .read_timeout_ms = SSE_HEARTBEAT_INTERVAL_MS }) catch |err| {
            log.err("subscriber_connect_failed", .{ .err = @errorName(err) });
            return error.SubscriberConnectFailed;
        };
        errdefer job.subscriber.deinit();
        job.subscriber.subscribe(job.channel()) catch |err| {
            log.err("subscriber_subscribe_failed", .{ .channel = job.channel(), .err = @errorName(err) });
            return error.SubscribeFailed;
        };
        return job;
    }

    fn channel(self: *const StreamJob) []const u8 {
        return self.channel_buf[0..self.channel_len];
    }

    fn destroy(self: *StreamJob) void {
        const alloc = self.ctx.alloc;
        self.subscriber.deinit();
        alloc.destroy(self);
    }
};

fn streamLoop(
    io: std.Io,
    alloc: std.mem.Allocator,
    subscriber: *redis_subscriber,
    stream: std.Io.net.Stream,
) !void {
    var seq: u64 = 0;
    var w = stream.writer(io, &.{});
    while (true) {
        const before_ms = clock.nowMillis();
        if (try subscriber.nextMessage()) |raw| {
            var msg = raw;
            defer msg.deinit(alloc);
            const kind = extractKind(msg.payload) orelse "message";
            try writeFrame(&w, seq, kind, msg.payload);
            seq +%= 1;
            continue;
        }
        // null = idle read timeout OR a closed/broken socket; the block time
        // tells them apart. A heartbeat write to a vanished client fails and
        // unwinds the loop, releasing the worker thread + Redis connection.
        switch (classifyIdle(clock.nowMillis() - before_ms)) {
            .close => return,
            .heartbeat => try w.interface.writeAll(SSE_HEARTBEAT_FRAME),
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

fn authorize(hx: Hx, workspace_id: []const u8, zombie_id: []const u8) bool {
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return false;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return false;
    }
    return verifyZombieInWorkspace(hx, conn, workspace_id, zombie_id);
}

fn verifyZombieInWorkspace(hx: Hx, conn: *pg.Conn, path_workspace_id: []const u8, zombie_id: []const u8) bool {
    var q = PgQuery.from(conn.query(
        "SELECT workspace_id::text FROM core.zombies WHERE id = $1::uuid",
        .{zombie_id},
    ) catch {
        common.internalDbError(hx.res, hx.req_id);
        return false;
    });
    defer q.deinit();
    const row = (q.next() catch {
        common.internalDbError(hx.res, hx.req_id);
        return false;
    }) orelse {
        hx.fail(ec.ERR_ZOMBIE_NOT_FOUND, ec.MSG_ZOMBIE_NOT_FOUND);
        return false;
    };
    const zombie_workspace = row.get([]const u8, 0) catch {
        common.internalDbError(hx.res, hx.req_id);
        return false;
    };
    if (!std.mem.eql(u8, path_workspace_id, zombie_workspace)) {
        hx.fail(ec.ERR_ZOMBIE_NOT_FOUND, ec.MSG_ZOMBIE_NOT_FOUND);
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
    // first per publisher contract.
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

test "classifyIdle: a near-instant null closes, a full idle window heartbeats" {
    try testing.expectEqual(IdleAction.close, classifyIdle(0));
    try testing.expectEqual(IdleAction.close, classifyIdle(SSE_TIMEOUT_MIN_ELAPSED_MS - 1));
    try testing.expectEqual(IdleAction.heartbeat, classifyIdle(SSE_TIMEOUT_MIN_ELAPSED_MS));
    try testing.expectEqual(IdleAction.heartbeat, classifyIdle(@as(i64, SSE_HEARTBEAT_INTERVAL_MS)));
}
