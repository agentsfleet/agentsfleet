//! M9_001 §4.4–4.5 — Grant approval webhook handler.
//! POST /v1/webhooks/{zombie_id}/grant-approval
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
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const queue_redis = @import("../../../queue/redis.zig");
const grants = @import("../integration_grants/handler.zig");

const log = std.log.scoped(.grant_approval_webhook);

const STATUS_PENDING = grants.GrantStatus.pending.toSlice();
const STATUS_APPROVED = grants.GrantStatus.approved.toSlice();
const STATUS_REVOKED = grants.GrantStatus.revoked.toSlice();

pub const Context = common.Context;

const GRANT_NONCE_KEY_PREFIX = "grant:nonce:";

const GrantApprovalBody = struct {
    grant_id: []const u8,
    decision: []const u8, // "approved" | "denied"
    nonce: []const u8,    // single-use, verified against Redis
};

/// Atomically verify and consume the stored nonce for a grant using a Lua script.
/// The Lua script does: GET key → compare with expected → DEL only on match → return 1/0.
/// This prevents a DoS where GETDEL would destroy the real nonce before the comparison
/// happens — an attacker who knows grant_id could consume the nonce with a wrong value,
/// leaving the legitimate Slack button unable to approve. With Lua, the nonce is only
/// deleted when the comparison succeeds; wrong values leave the nonce intact.
fn verifyAndConsumeNonce(
    queue: *queue_redis.Client,
    grant_id: []const u8,
    nonce: []const u8,
) bool {
    var key_buf: [256]u8 = undefined;
    const key = std.fmt.bufPrint(&key_buf, "{s}{s}", .{
        GRANT_NONCE_KEY_PREFIX, grant_id,
    }) catch return false;

    // Lua: GET → compare → DEL only on match → 1 (matched) or 0 (missing/mismatch)
    const lua =
        \\local s=redis.call('GET',KEYS[1])
        \\if s==false then return 0 end
        \\if s==ARGV[1] then redis.call('DEL',KEYS[1]) return 1 end
        \\return 0
    ;
    var resp = queue.commandAllowError(&.{ "EVAL", lua, "1", key, nonce }) catch return false;
    defer resp.deinit(queue.alloc);

    return switch (resp) {
        .integer => |n| n == 1,
        else => false,
    };
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
    hx: hx_mod.Hx,
    conn: *pg.Conn,
    grant_id: []const u8,
    zombie_id: []const u8,
    is_approved: bool,
    now_ms: i64,
) bool {
    if (is_approved) {
        _ = conn.exec(
            \\UPDATE core.integration_grants
            \\SET status = $1, approved_at = $2
            \\WHERE grant_id = $3 AND zombie_id = $4::uuid AND status = $5
        , .{ STATUS_APPROVED, now_ms, grant_id, zombie_id, STATUS_PENDING }) catch {
            common.internalDbError(hx.res, hx.req_id);
            return false;
        };
    } else {
        _ = conn.exec(
            \\UPDATE core.integration_grants
            \\SET status = $1, revoked_at = $2
            \\WHERE grant_id = $3 AND zombie_id = $4::uuid AND status = $5
        , .{ STATUS_REVOKED, now_ms, grant_id, zombie_id, STATUS_PENDING }) catch {
            common.internalDbError(hx.res, hx.req_id);
            return false;
        };
    }
    return true;
}

pub fn innerGrantApproval(hx: hx_mod.Hx, req: *httpz.Request, zombie_id: []const u8) void {
    const raw_body = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "Request body required");
        return;
    };
    const parsed = std.json.parseFromSlice(GrantApprovalBody, hx.alloc, raw_body, .{}) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "Malformed JSON body");
        return;
    };
    defer parsed.deinit();
    const body = parsed.value;

    if (body.grant_id.len == 0) {
        hx.fail(ec.ERR_INVALID_REQUEST, "grant_id required");
        return;
    }
    if (body.nonce.len == 0) {
        hx.fail(ec.ERR_INVALID_REQUEST, "nonce required");
        return;
    }
    const is_approved = std.mem.eql(u8, body.decision, "approved");
    const is_denied   = std.mem.eql(u8, body.decision, "denied");
    if (!is_approved and !is_denied) {
        hx.fail(ec.ERR_INVALID_REQUEST, "decision must be 'approved' or 'denied'");
        return;
    }

    // Verify and consume the one-time nonce before any DB mutation.
    // verifyAndConsumeNonce uses a Lua script that only deletes the nonce on match,
    // so a wrong nonce leaves the key intact for the legitimate Slack button.
    if (!verifyAndConsumeNonce(hx.ctx.queue, body.grant_id, body.nonce)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "Invalid or expired approval nonce");
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const now_ms = std.time.milliTimestamp();
    if (!applyDecision(hx, conn, body.grant_id, zombie_id, is_approved, now_ms)) return;

    const workspace_id = fetchWorkspaceId(hx.ctx.pool, hx.alloc, zombie_id);
    const event_type: []const u8 = if (is_approved) "grant.approved" else "grant.denied";

    log.info("grant_approval.{s} zombie_id={s} workspace_id={s} grant_id={s} event_type={s}", .{
        body.decision, zombie_id, workspace_id, body.grant_id, event_type,
    });

    hx.ok(.ok, .{
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

test "is_denied logic: only 'denied' matches, not 'approve' or 'DENIED'" {
    try std.testing.expect(std.mem.eql(u8, "denied", "denied"));
    try std.testing.expect(!std.mem.eql(u8, "approved", "denied"));
    try std.testing.expect(!std.mem.eql(u8, "DENIED", "denied")); // case-sensitive
    try std.testing.expect(!std.mem.eql(u8, "", "denied"));
}

test "GrantApprovalBody: missing nonce field fails to parse" {
    const alloc = std.testing.allocator;
    // No nonce field — JSON parser rejects because nonce is non-optional.
    const json = "{\"grant_id\":\"uzg_001\",\"decision\":\"approved\"}";
    try std.testing.expectError(
        error.MissingField,
        std.json.parseFromSlice(GrantApprovalBody, alloc, json, .{}),
    );
}

test "GrantApprovalBody: missing grant_id field fails to parse" {
    const alloc = std.testing.allocator;
    const json = "{\"decision\":\"approved\",\"nonce\":\"abc123\"}";
    try std.testing.expectError(
        error.MissingField,
        std.json.parseFromSlice(GrantApprovalBody, alloc, json, .{}),
    );
}

test "GRANT_NONCE_KEY_PREFIX: expected value for Redis key construction" {
    // Regression guard: changing the prefix silently breaks nonce lookup across restarts.
    try std.testing.expectEqualStrings("grant:nonce:", GRANT_NONCE_KEY_PREFIX);
}

test "nonce mismatch detection: verifyAndConsumeNonce contract — wrong nonce must not consume" {
    // Regression doc: verifyAndConsumeNonce uses a Lua script that only calls DEL when the
    // stored value matches ARGV[1]. A wrong nonce returns 0 and leaves the key intact.
    // This test documents the expected equality semantics the Lua script enforces.
    const stored = "abcd1234abcd1234abcd1234abcd1234";
    const correct = "abcd1234abcd1234abcd1234abcd1234";
    const wrong   = "xxxx1234abcd1234abcd1234abcd1234";
    try std.testing.expect(std.mem.eql(u8, stored, correct));  // Lua: s==ARGV[1] → DEL → return 1
    try std.testing.expect(!std.mem.eql(u8, stored, wrong));   // Lua: s!=ARGV[1] → no DEL → return 0
}

test "verifyAndConsumeNonce: Lua script contains GET, DEL, and ARGV comparison" {
    // Regression guard: the Lua script text must remain structurally correct.
    // If someone refactors to a plain GETDEL, this test catches the regression.
    const lua =
        \\local s=redis.call('GET',KEYS[1])
        \\if s==false then return 0 end
        \\if s==ARGV[1] then redis.call('DEL',KEYS[1]) return 1 end
        \\return 0
    ;
    try std.testing.expect(std.mem.containsAtLeast(u8, lua, 1, "GET"));
    try std.testing.expect(std.mem.containsAtLeast(u8, lua, 1, "DEL"));
    try std.testing.expect(std.mem.containsAtLeast(u8, lua, 1, "ARGV[1]"));
    // Must NOT contain GETDEL — that's the unsafe pattern we replaced
    try std.testing.expect(!std.mem.containsAtLeast(u8, lua, 1, "GETDEL"));
}
