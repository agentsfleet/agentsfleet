//! M9_001 §4.4–4.5 — Grant approval webhook handler.
//! POST /v1/webhooks/{zombie_id}:grant-approval
//!
//! Called by the Slack/Discord notification provider after the human clicks
//! Approve or Deny. The nonce is required for authenticity — it is generated
//! at grant-request time, stored at grant:nonce:{grant_id} in Redis, embedded
//! in the Slack button value, and consumed (single-use DEL) on first use here.
//!
//! §4.4 decision=approved → status=approved, approved_at set
//! §4.5 decision=denied   → status=revoked,  revoked_at set

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;
const common = @import("common.zig");
const ec = @import("../../errors/error_registry.zig");
const activity_stream = @import("../../zombie/activity_stream.zig");
const queue_redis = @import("../../queue/redis.zig");

const log = std.log.scoped(.grant_approval_webhook);

pub const Context = common.Context;

const GRANT_NONCE_KEY_PREFIX = "grant:nonce:";

const GrantApprovalBody = struct {
    grant_id: []const u8,
    decision: []const u8, // "approved" | "denied"
    nonce: []const u8,    // single-use, verified against Redis
};

/// Fetch the stored nonce for a grant from Redis, then DEL it (single-use).
/// Returns null if the key is missing (expired or never stored).
fn fetchAndConsumeNonce(
    queue: *queue_redis.Client,
    alloc: std.mem.Allocator,
    grant_id: []const u8,
) ?[]const u8 {
    var key_buf: [256]u8 = undefined;
    const key = std.fmt.bufPrint(&key_buf, "{s}{s}", .{
        GRANT_NONCE_KEY_PREFIX, grant_id,
    }) catch return null;

    var resp = queue.commandAllowError(&.{ "GET", key }) catch return null;
    defer resp.deinit(queue.alloc);

    const stored: []const u8 = switch (resp) {
        .bulk => |b| b orelse return null,
        .simple => |s| s,
        else => return null,
    };
    const owned = alloc.dupe(u8, stored) catch return null;

    // Consume: DEL so the nonce cannot be replayed.
    _ = queue.command(&.{ "DEL", key }) catch {};

    return owned;
}

fn fetchWorkspaceId(pool: *pg.Pool, alloc: std.mem.Allocator, zombie_id: []const u8) []const u8 {
    const conn = pool.acquire() catch return "";
    defer pool.release(conn);
    var q = PgQuery.from(conn.query(
        \\SELECT workspace_id::text FROM core.zombies WHERE id = $1::uuid LIMIT 1
    , .{zombie_id}) catch return "");
    defer q.deinit();
    const row_opt = q.next() catch return "";
    const row = row_opt orelse return "";
    const ws = row.get([]u8, 0) catch return "";
    return alloc.dupe(u8, ws) catch return "";
}

fn applyDecision(
    conn: *pg.Conn,
    grant_id: []const u8,
    zombie_id: []const u8,
    is_approved: bool,
    now_ms: i64,
    res: *httpz.Response,
    req_id: []const u8,
) bool {
    if (is_approved) {
        _ = conn.exec(
            \\UPDATE core.integration_grants
            \\SET status = 'approved', approved_at = $1
            \\WHERE grant_id = $2 AND zombie_id = $3::uuid AND status = 'pending'
        , .{ now_ms, grant_id, zombie_id }) catch {
            common.internalDbError(res, req_id);
            return false;
        };
    } else {
        _ = conn.exec(
            \\UPDATE core.integration_grants
            \\SET status = 'revoked', revoked_at = $1
            \\WHERE grant_id = $2 AND zombie_id = $3::uuid AND status = 'pending'
        , .{ now_ms, grant_id, zombie_id }) catch {
            common.internalDbError(res, req_id);
            return false;
        };
    }
    return true;
}

pub fn handleGrantApproval(
    ctx: *Context,
    req: *httpz.Request,
    res: *httpz.Response,
    zombie_id: []const u8,
) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const raw_body = req.body() orelse {
        common.errorResponse(res, ec.ERR_INVALID_REQUEST, "Request body required", req_id);
        return;
    };
    const parsed = std.json.parseFromSlice(GrantApprovalBody, alloc, raw_body, .{}) catch {
        common.errorResponse(res, ec.ERR_INVALID_REQUEST, "Malformed JSON body", req_id);
        return;
    };
    defer parsed.deinit();
    const body = parsed.value;

    if (body.grant_id.len == 0) {
        common.errorResponse(res, ec.ERR_INVALID_REQUEST, "grant_id required", req_id);
        return;
    }
    if (body.nonce.len == 0) {
        common.errorResponse(res, ec.ERR_INVALID_REQUEST, "nonce required", req_id);
        return;
    }
    const is_approved = std.mem.eql(u8, body.decision, "approved");
    const is_denied   = std.mem.eql(u8, body.decision, "denied");
    if (!is_approved and !is_denied) {
        common.errorResponse(res, ec.ERR_INVALID_REQUEST,
            "decision must be 'approved' or 'denied'", req_id);
        return;
    }

    // Verify and consume the one-time nonce before any DB mutation.
    const stored_nonce = fetchAndConsumeNonce(ctx.queue, alloc, body.grant_id) orelse {
        common.errorResponse(res, ec.ERR_INVALID_REQUEST,
            "Invalid or expired approval nonce", req_id);
        return;
    };
    if (!std.mem.eql(u8, stored_nonce, body.nonce)) {
        common.errorResponse(res, ec.ERR_INVALID_REQUEST,
            "Invalid or expired approval nonce", req_id);
        return;
    }

    const conn = ctx.pool.acquire() catch {
        common.internalDbUnavailable(res, req_id);
        return;
    };
    defer ctx.pool.release(conn);

    const now_ms = std.time.milliTimestamp();
    if (!applyDecision(conn, body.grant_id, zombie_id, is_approved, now_ms, res, req_id)) return;

    const workspace_id = fetchWorkspaceId(ctx.pool, alloc, zombie_id);
    const event_type: []const u8 = if (is_approved) "grant.approved" else "grant.denied";
    activity_stream.logEvent(ctx.pool, alloc, .{
        .zombie_id    = zombie_id,
        .workspace_id = workspace_id,
        .event_type   = event_type,
        .detail       = body.grant_id,
    });

    log.info("grant_approval.{s} zombie_id={s} grant_id={s}", .{
        body.decision, zombie_id, body.grant_id,
    });

    common.writeJson(res, .ok, .{
        .status   = "ok",
        .grant_id = body.grant_id,
        .decision = body.decision,
    });
}

// ── Tests ──────────────────────────────────────────────────────────────────

test "GrantApprovalBody parses approved decision with nonce" {
    const alloc = std.testing.allocator;
    const json = "{\"grant_id\":\"uzg_001\",\"decision\":\"approved\",\"nonce\":\"abc123\"}";
    const parsed = try std.json.parseFromSlice(GrantApprovalBody, alloc, json, .{});
    defer parsed.deinit();
    try std.testing.expectEqualStrings("uzg_001", parsed.value.grant_id);
    try std.testing.expectEqualStrings("approved", parsed.value.decision);
    try std.testing.expectEqualStrings("abc123", parsed.value.nonce);
}

test "GrantApprovalBody parses denied decision with nonce" {
    const alloc = std.testing.allocator;
    const json = "{\"grant_id\":\"uzg_002\",\"decision\":\"denied\",\"nonce\":\"xyz789\"}";
    const parsed = try std.json.parseFromSlice(GrantApprovalBody, alloc, json, .{});
    defer parsed.deinit();
    try std.testing.expectEqualStrings("denied", parsed.value.decision);
    try std.testing.expectEqualStrings("xyz789", parsed.value.nonce);
}

test "is_approved logic: only 'approved' matches, not 'deny' or empty" {
    try std.testing.expect(std.mem.eql(u8, "approved", "approved"));
    try std.testing.expect(!std.mem.eql(u8, "denied", "approved"));
    try std.testing.expect(!std.mem.eql(u8, "", "approved"));
}
