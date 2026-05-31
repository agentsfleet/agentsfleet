//! GET /v1/workspaces/{ws}/zombies/{id}/events/stream — Server-Sent
//! Events tail of the Redis pub/sub channel `zombie:{id}:activity`.
//!
//! Connection lifecycle:
//!   1. Authorize (Bearer middleware + path-workspace ownership).
//!   2. Issue `SUBSCRIBE zombie:{id}:activity` on a dedicated Redis
//!      connection — pub/sub blocks the conn, so we can NOT share the
//!      request-handler queue client.
//!   3. Hand the TCP stream to the handler via `startEventStreamSync`.
//!   4. Loop: read pub/sub message → write one SSE frame.
//!   5. On client disconnect (write error) or any read error, close.
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
const httpz = @import("httpz");
const pg = @import("pg");
const logging = @import("log");

const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const redis_subscriber = @import("../../../queue/redis_subscriber.zig");
const redis_types = @import("../../../queue/redis_types.zig");

const log = logging.scoped(.http_zombie_events_stream);

const Hx = hx_mod.Hx;

const channel_prefix = "zombie:";
const channel_suffix = ":activity";

/// Idle wake-up cadence for the SSE subscriber. Each tick with no pub/sub
/// traffic sends a heartbeat comment so a vanished client is detected by the
/// failing write — without it the worker parks on the Redis read forever,
/// holding an httpz worker + a Redis connection until a publish that may never
/// come (dead client + idle zombie = a wedged worker, eventually pool-starving).
const SSE_HEARTBEAT_INTERVAL_MS: u32 = 15_000;
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

    if (!authorize(hx, workspace_id, zombie_id)) return;

    var subscriber = redis_subscriber.connectFromEnv(hx.alloc, redis_types.RedisRole.api, .{ .read_timeout_ms = SSE_HEARTBEAT_INTERVAL_MS }) catch |err| {
        log.err("subscriber_connect_failed", .{ .err = @errorName(err) });
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer subscriber.deinit();

    var channel_buf: [128]u8 = undefined;
    const channel = std.fmt.bufPrint(&channel_buf, "{s}{s}{s}", .{ channel_prefix, zombie_id, channel_suffix }) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    subscriber.subscribe(channel) catch |err| {
        log.err("subscriber_subscribe_failed", .{ .channel = channel, .err = @errorName(err) });
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };

    const stream = hx.res.startEventStreamSync() catch |err| {
        log.warn("sse_start_failed", .{ .err = @errorName(err) });
        return;
    };

    streamLoop(hx.alloc, &subscriber, stream) catch |err| {
        // Most "errors" here are client disconnects mid-write (broken pipe).
        // Log at debug — the operator-visible event is the connection close,
        // not the inner write error.
        log.debug("sse_stream_loop_exit", .{ .err = @errorName(err) });
    };

    subscriber.unsubscribe(channel);
}

fn streamLoop(
    alloc: std.mem.Allocator,
    subscriber: *redis_subscriber,
    stream: std.net.Stream,
) !void {
    var seq: u64 = 0;
    var w = stream.writer(&.{});
    while (true) {
        const before_ms = std.time.milliTimestamp();
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
        switch (classifyIdle(std.time.milliTimestamp() - before_ms)) {
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
