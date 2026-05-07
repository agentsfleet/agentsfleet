// POST /v1/workspaces/{ws}/zombies/{id}/messages — operator-driven chat ingress.
//
// Verifies workspace ownership, normalizes the body into an EventEnvelope,
// XADDs onto zombie:{id}:events, and returns 202 with the Redis-assigned
// event_id. The CLI uses event_id to filter SSE frames for the message it
// just sent. No SETEX, no GETDEL, no synthetic-event injection in the
// worker — the event hits the same single-ingress stream every other
// producer (webhook, cron, continuation) writes to.
//
// RULE FLS: all conn.query() calls use PgQuery with defer deinit().
// RULE NSQ: schema-qualified SQL (core.zombies).

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const logging = @import("log");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const EventEnvelope = @import("../../../zombie/event_envelope.zig");

const log = logging.scoped(.zombie_messages);

const Hx = hx_mod.Hx;

const MAX_MESSAGE_LEN: usize = 8192;

const MessageBody = struct {
    message: []const u8,
};

// ── Handler ───────────────────────────────────────────────────────────────────

pub fn innerZombieMessagesPost(hx: Hx, req: *httpz.Request, workspace_id: []const u8, zombie_id: []const u8) void {
    const body_raw = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "request body required");
        return;
    };
    const parsed = std.json.parseFromSlice(
        MessageBody,
        hx.alloc,
        body_raw,
        .{ .ignore_unknown_fields = true },
    ) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_MALFORMED_JSON);
        return;
    };
    const msg = parsed.value.message;
    if (msg.len == 0) {
        hx.fail(ec.ERR_INVALID_REQUEST, "message must not be empty");
        return;
    }
    if (msg.len > MAX_MESSAGE_LEN) {
        hx.fail(ec.ERR_INVALID_REQUEST, "message must not exceed 8192 bytes");
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return;
    }

    if (!verifyZombieInWorkspace(hx, conn, workspace_id, zombie_id)) return;

    const request_json = std.fmt.allocPrint(
        hx.alloc,
        "{{\"message\":{f}}}",
        .{std.json.fmt(msg, .{})},
    ) catch {
        common.internalOperationError(hx.res, "OOM building steer request payload", hx.req_id);
        return;
    };
    defer hx.alloc.free(request_json);

    const actor = buildSteerActor(hx.alloc, hx.principal) catch {
        common.internalOperationError(hx.res, "OOM building steer actor", hx.req_id);
        return;
    };
    defer hx.alloc.free(actor);

    const envelope = EventEnvelope{
        .event_id = "",
        .zombie_id = zombie_id,
        .workspace_id = workspace_id,
        .actor = actor,
        .event_type = .chat,
        .request_json = request_json,
        .created_at = std.time.milliTimestamp(),
    };

    const event_id = hx.ctx.queue.xaddZombieEvent(envelope) catch |err| {
        log.warn("xadd_failed", .{ .zombie_id = zombie_id, .actor = actor, .err = @errorName(err) });
        common.internalOperationError(hx.res, "failed to enqueue chat event", hx.req_id);
        return;
    };
    defer hx.ctx.alloc.free(event_id);

    log.info("posted", .{ .zombie_id = zombie_id, .event_id = event_id, .actor = actor });
    hx.ok(.accepted, .{
        .status = "accepted",
        .event_id = event_id,
    });
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Verify zombie exists and belongs to the path workspace. Writes a 404 on
/// mismatch (do not leak existence across workspaces) and returns false.
fn verifyZombieInWorkspace(
    hx: Hx,
    conn: *pg.Conn,
    path_workspace_id: []const u8,
    zombie_id: []const u8,
) bool {
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

/// Build the `steer:<user>` actor string. OIDC principals carry a stable
/// `user_id`; api-key principals fall back to a flat `steer:api` so the
/// dashboard can still group by source category.
fn buildSteerActor(alloc: std.mem.Allocator, principal: common.AuthPrincipal) ![]u8 {
    if (principal.user_id) |uid| {
        return std.fmt.allocPrint(alloc, "steer:{s}", .{uid});
    }
    return alloc.dupe(u8, "steer:api");
}
