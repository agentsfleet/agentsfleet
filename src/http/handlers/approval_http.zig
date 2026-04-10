// POST /v1/webhooks/{zombie_id}:approval — Slack interactive payload callback.
//
// Receives Slack button clicks (approve/deny) for the approval gate.
// Validates the payload, resolves the pending action in Redis, and
// records the decision in the audit table.
//
// Must respond to Slack within 3 seconds — processing is inline since
// it's just a Redis SET + Postgres INSERT.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");
const common = @import("common.zig");
const ec = @import("../../errors/codes.zig");
const approval_gate = @import("../../zombie/approval_gate.zig");
const activity_stream = @import("../../zombie/activity_stream.zig");

const log = std.log.scoped(.http_approval);

pub const Context = common.Context;

const ApprovalDecision = enum {
    approve,
    deny,

    pub fn toConstString(self: ApprovalDecision) []const u8 {
        return switch (self) {
            .approve => ec.GATE_DECISION_APPROVE,
            .deny => ec.GATE_DECISION_DENY,
        };
    }
};

const ApprovalPayload = struct {
    action_id: []const u8,
    decision: ApprovalDecision,
};

pub fn handleApprovalCallback(
    ctx: *Context,
    req: *httpz.Request,
    res: *httpz.Response,
    zombie_id: []const u8,
) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    // Verify request signature (HMAC-SHA256) if signing secret is configured.
    // Without this, any actor who knows the URL can submit approve/deny.
    if (!verifyRequestSignature(req, res, req_id)) return;

    const payload = parseApprovalBody(alloc, req, res, req_id) orelse return;
    const decision_str = payload.decision.toConstString();

    // Check that the pending action exists in Redis
    var pending_key_buf: [256]u8 = undefined;
    const pending_key = std.fmt.bufPrint(&pending_key_buf, "{s}{s}:{s}", .{
        ec.GATE_PENDING_KEY_PREFIX, zombie_id, payload.action_id,
    }) catch {
        common.internalOperationError(res, "key overflow", req_id);
        return;
    };

    const exists = ctx.queue.exists(pending_key) catch {
        log.err("approval.redis_check_fail zombie_id={s} action_id={s}", .{ zombie_id, payload.action_id });
        common.internalOperationError(res, "Redis unavailable", req_id);
        return;
    };
    if (!exists) {
        common.errorResponse(res, .not_found, ec.ERR_APPROVAL_NOT_FOUND, ec.MSG_APPROVAL_NOT_FOUND, req_id);
        return;
    }

    // Write the decision to Redis (unblocks waitForDecision)
    approval_gate.resolveApproval(ctx.queue, payload.action_id, decision_str) catch {
        log.err("approval.resolve_fail zombie_id={s} action_id={s}", .{ zombie_id, payload.action_id });
        common.internalOperationError(res, "Failed to resolve approval", req_id);
        return;
    };

    // Fetch workspace_id for activity logging
    const workspace_id = fetchWorkspaceId(ctx.pool, alloc, zombie_id) catch "";

    // Record in audit table
    const event_type: []const u8 = switch (payload.decision) {
        .approve => ec.GATE_EVENT_APPROVED,
        .deny => ec.GATE_EVENT_DENIED,
    };

    approval_gate.recordGateDecision(
        ctx.pool,
        alloc,
        zombie_id,
        workspace_id,
        payload.action_id,
        "",
        "",
        decision_str,
        "",
    );

    // Log to activity stream
    if (workspace_id.len > 0) {
        activity_stream.logEvent(ctx.pool, alloc, .{
            .zombie_id = zombie_id,
            .workspace_id = workspace_id,
            .event_type = event_type,
            .detail = payload.action_id,
        });
    }

    log.info("approval.resolved zombie_id={s} action_id={s} decision={s}", .{
        zombie_id, payload.action_id, decision_str,
    });

    res.status = 200;
    res.json(.{
        .status = "resolved",
        .action_id = payload.action_id,
        .decision = decision_str,
    }, .{}) catch {};
}

const RawApprovalPayload = struct {
    action_id: []const u8,
    decision: []const u8,
};

fn parseApprovalBody(
    alloc: std.mem.Allocator,
    req: *httpz.Request,
    res: *httpz.Response,
    req_id: []const u8,
) ?ApprovalPayload {
    const body = req.body() orelse {
        common.errorResponse(res, .bad_request, ec.ERR_APPROVAL_PARSE_FAILED, ec.MSG_APPROVAL_INVALID_BODY, req_id);
        return null;
    };
    if (!common.checkBodySize(req, res, body, req_id)) return null;
    const parsed = std.json.parseFromSlice(RawApprovalPayload, alloc, body, .{ .ignore_unknown_fields = true }) catch {
        common.errorResponse(res, .bad_request, ec.ERR_APPROVAL_PARSE_FAILED, ec.MSG_APPROVAL_INVALID_BODY, req_id);
        return null;
    };
    const raw = parsed.value;
    if (raw.action_id.len == 0 or raw.decision.len == 0) {
        common.errorResponse(res, .bad_request, ec.ERR_APPROVAL_PARSE_FAILED, ec.MSG_APPROVAL_INVALID_BODY, req_id);
        return null;
    }
    const decision: ApprovalDecision = if (std.mem.eql(u8, raw.decision, ec.GATE_DECISION_APPROVE))
        .approve
    else if (std.mem.eql(u8, raw.decision, ec.GATE_DECISION_DENY))
        .deny
    else {
        common.errorResponse(res, .bad_request, ec.ERR_APPROVAL_PARSE_FAILED, ec.MSG_APPROVAL_INVALID_DECISION, req_id);
        return null;
    };
    return ApprovalPayload{ .action_id = raw.action_id, .decision = decision };
}

/// Verify the request signature using HMAC-SHA256.
/// Expects X-Signature-Timestamp and X-Signature headers.
/// Signing secret is read from APPROVAL_SIGNING_SECRET env var.
/// If no signing secret is configured, rejects all requests (fail-closed).
fn verifyRequestSignature(req: *httpz.Request, res: *httpz.Response, req_id: []const u8) bool {
    const secret = std.process.getEnvVarOwned(std.heap.page_allocator, "APPROVAL_SIGNING_SECRET") catch {
        // No signing secret configured — reject (fail-closed, no insecure fallback)
        log.warn("approval.no_signing_secret_configured req_id={s}", .{req_id});
        common.errorResponse(res, .forbidden, ec.ERR_APPROVAL_INVALID_SIGNATURE, "Signing secret not configured", req_id);
        return false;
    };
    defer std.heap.page_allocator.free(secret);

    const timestamp = req.header("x-signature-timestamp") orelse {
        common.errorResponse(res, .unauthorized, ec.ERR_APPROVAL_INVALID_SIGNATURE, "Missing signature timestamp", req_id);
        return false;
    };
    const provided_sig = req.header("x-signature") orelse {
        common.errorResponse(res, .unauthorized, ec.ERR_APPROVAL_INVALID_SIGNATURE, "Missing signature", req_id);
        return false;
    };

    const body = req.body() orelse "";

    // Compute HMAC-SHA256("v0:" ++ timestamp ++ ":" ++ body)
    const HmacSha256 = std.crypto.auth.hmac.sha2.HmacSha256;
    var mac: [HmacSha256.mac_length]u8 = undefined;
    var h = HmacSha256.init(secret);
    h.update("v0:");
    h.update(timestamp);
    h.update(":");
    h.update(body);
    h.final(&mac);

    // Format as "v0=" ++ hex(mac)
    var expected_buf: [3 + HmacSha256.mac_length * 2]u8 = undefined;
    @memcpy(expected_buf[0..3], "v0=");
    const hex_chars = "0123456789abcdef";
    for (mac, 0..) |byte, i| {
        expected_buf[3 + i * 2] = hex_chars[byte >> 4];
        expected_buf[3 + i * 2 + 1] = hex_chars[byte & 0x0f];
    }
    const expected = expected_buf[0 .. 3 + HmacSha256.mac_length * 2];

    // Constant-time comparison (Rule 4: no short-circuit for secrets)
    if (provided_sig.len != expected.len) {
        common.errorResponse(res, .unauthorized, ec.ERR_APPROVAL_INVALID_SIGNATURE, "Invalid signature", req_id);
        return false;
    }
    var diff: u8 = 0;
    for (provided_sig, expected) |a, b| diff |= a ^ b;
    if (diff != 0) {
        common.errorResponse(res, .unauthorized, ec.ERR_APPROVAL_INVALID_SIGNATURE, "Invalid signature", req_id);
        return false;
    }

    return true;
}

fn fetchWorkspaceId(pool: *pg.Pool, alloc: std.mem.Allocator, zombie_id: []const u8) ![]const u8 {
    const conn = try pool.acquire();
    defer pool.release(conn);
    var q = try conn.query( // check-pg-drain: ok — drain called below
        \\SELECT workspace_id::text FROM core.zombies WHERE id = $1::uuid
    , .{zombie_id});
    defer q.deinit();
    const row = try q.next() orelse {
        q.drain() catch {};
        return "";
    };
    const ws = try alloc.dupe(u8, try row.get([]const u8, 0));
    q.drain() catch {};
    return ws;
}

test "ApprovalDecision.toConstString maps correctly" {
    try std.testing.expectEqualStrings("approve", ApprovalDecision.approve.toConstString());
    try std.testing.expectEqualStrings("deny", ApprovalDecision.deny.toConstString());
}
