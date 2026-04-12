// POST /v1/slack/interactions — Slack button callback receiver.
//
// Receives Slack block_actions payloads (Approve/Deny button clicks).
// After signature verification, extracts the action_id from the button,
// parses zombie_id + gate action_id + decision, and delegates to the
// approval gate (resolveApproval).
//
// action_id format (from approval_gate_slack.zig):
//   gate_{zombie_id}_{action_id}_{decision}
//
// Routing:
//   1. Verify x-slack-signature (HMAC-SHA256, RULE CTM) → UZ-WH-010 on fail
//   2. Verify x-slack-request-timestamp freshness → UZ-WH-011 on stale
//   3. Parse URL-encoded body: payload= field contains JSON
//   4. Extract actions[0].action_id
//   5. Parse action_id: gate_{zombie_id}_{inner_id}_{decision}
//   6. Call approval_gate.resolveApproval
//   7. HTTP 200 (always respond quickly — Slack has 3s timeout)
//
// Does NOT use hx.authenticated() — signed by Slack signing secret.
// Env var: SLACK_SIGNING_SECRET

const std = @import("std");
const httpz = @import("httpz");
const webhook_verify = @import("../../zombie/webhook_verify.zig");
const approval_gate = @import("../../zombie/approval_gate.zig");
const common = @import("common.zig");
const ec = @import("../../errors/error_registry.zig");

const log = std.log.scoped(.http_slack_interactions);

pub const Context = common.Context;

const GATE_PREFIX = "gate_";
const GRANT_PREFIX = "grant_";

const SlackInteractionPayload = struct {
    type: []const u8,
    actions: ?[]const SlackAction = null,
};

const SlackAction = struct {
    action_id: []const u8,
};

// ── Handler ───────────────────────────────────────────────────────────────────

pub fn handleInteraction(ctx: *Context, req: *httpz.Request, res: *httpz.Response) void {
    var arena = std.heap.ArenaAllocator.init(ctx.alloc);
    defer arena.deinit();
    const alloc = arena.allocator();
    const req_id = common.requestId(alloc);

    const raw_body = req.body() orelse {
        common.errorResponse(res, ec.ERR_INVALID_REQUEST, "Empty body", req_id);
        return;
    };

    if (!verifySlackSignature(req, raw_body, res, req_id)) return;

    // Slack sends interactions as URL-encoded: payload={url_encoded_json}
    const payload_json = extractPayloadField(alloc, raw_body) orelse {
        log.warn("slack.interactions.no_payload req_id={s}", .{req_id});
        common.errorResponse(res, ec.ERR_INVALID_REQUEST, "Missing payload field", req_id);
        return;
    };

    const parsed = std.json.parseFromSlice(SlackInteractionPayload, alloc, payload_json, .{ .ignore_unknown_fields = true }) catch {
        log.warn("slack.interactions.parse_fail req_id={s}", .{req_id});
        common.errorResponse(res, ec.ERR_INVALID_REQUEST, "Invalid interaction payload", req_id);
        return;
    };
    defer parsed.deinit();

    const actions = parsed.value.actions orelse {
        log.warn("slack.interactions.no_actions req_id={s}", .{req_id});
        res.status = 200;
        res.body = "{}";
        return;
    };
    if (actions.len == 0) {
        res.status = 200;
        res.body = "{}";
        return;
    }

    const action_id = actions[0].action_id;

    if (std.mem.startsWith(u8, action_id, GATE_PREFIX)) {
        handleGateAction(ctx, alloc, action_id[GATE_PREFIX.len..], res, req_id);
    } else if (std.mem.startsWith(u8, action_id, GRANT_PREFIX)) {
        // Integration grant approval — M9 territory. Log and ack for now.
        log.info("slack.interactions.grant_action action_id={s} req_id={s}", .{ action_id, req_id });
        res.status = 200;
        res.body = "{}";
    } else {
        log.warn("slack.interactions.unknown_action action_id={s} req_id={s}", .{ action_id, req_id });
        res.status = 200;
        res.body = "{}";
    }
}

// ── Internal ─────────────────────────────────────────────────────────────────

/// Route gate_{zombie_id}_{inner_action_id}_{decision} to approval gate.
fn handleGateAction(ctx: *Context, alloc: std.mem.Allocator, rest: []const u8, res: *httpz.Response, req_id: []const u8) void {
    // rest = {zombie_id}_{inner_action_id}_{decision}
    // zombie_id is a UUID v7 (36 chars). Find by fixed length.
    const UUID_LEN = 36;
    if (rest.len < UUID_LEN + 2) {
        log.warn("slack.interactions.gate_short action rest={s} req_id={s}", .{ rest, req_id });
        res.status = 200;
        res.body = "{}";
        return;
    }
    // zombie_id ends at position UUID_LEN, must be followed by '_'
    if (rest[UUID_LEN] != '_') {
        log.warn("slack.interactions.gate_bad_sep req_id={s}", .{req_id});
        res.status = 200;
        res.body = "{}";
        return;
    }
    const zombie_id = rest[0..UUID_LEN];
    const after_zombie = rest[UUID_LEN + 1 ..];

    // Find decision at the end: last segment after final '_'
    const last_underscore = std.mem.lastIndexOfScalar(u8, after_zombie, '_') orelse {
        log.warn("slack.interactions.gate_no_decision req_id={s}", .{req_id});
        res.status = 200;
        res.body = "{}";
        return;
    };
    const inner_action_id = after_zombie[0..last_underscore];
    const decision = after_zombie[last_underscore + 1 ..];

    if (!std.mem.eql(u8, decision, "approve") and !std.mem.eql(u8, decision, "deny")) {
        log.warn("slack.interactions.invalid_decision decision={s} req_id={s}", .{ decision, req_id });
        res.status = 200;
        res.body = "{}";
        return;
    }

    // Check pending gate exists
    var pending_key_buf: [256]u8 = undefined;
    const pending_key = std.fmt.bufPrint(&pending_key_buf, "{s}{s}:{s}", .{
        ec.GATE_PENDING_KEY_PREFIX, zombie_id, inner_action_id,
    }) catch {
        common.internalOperationError(res, "key overflow", req_id);
        return;
    };
    const exists = ctx.queue.exists(pending_key) catch {
        log.err("slack.interactions.redis_fail req_id={s}", .{req_id});
        res.status = 200;
        res.body = "{}";
        return;
    };
    if (!exists) {
        log.debug("slack.interactions.gate_not_found action_id={s} req_id={s}", .{ inner_action_id, req_id });
        res.status = 200;
        res.body = "{\"error\":\"gate_expired\"}";
        return;
    }

    approval_gate.resolveApproval(ctx.queue, inner_action_id, decision) catch |err| {
        log.err("slack.interactions.resolve_fail err={s} req_id={s}", .{ @errorName(err), req_id });
        res.status = 200;
        res.body = "{}";
        return;
    };

    _ = alloc;
    log.info("slack.interactions.gate_resolved zombie_id={s} action_id={s} decision={s} req_id={s}", .{
        zombie_id, inner_action_id, decision, req_id,
    });
    res.status = 200;
    res.body = "{}";
}

/// Verify Slack request signature and timestamp freshness (RULE CTM).
fn verifySlackSignature(req: *httpz.Request, body: []const u8, res: *httpz.Response, req_id: []const u8) bool {
    const secret = std.process.getEnvVarOwned(std.heap.page_allocator, "SLACK_SIGNING_SECRET") catch {
        log.err("slack.interactions.no_signing_secret req_id={s}", .{req_id});
        common.errorResponse(res, ec.ERR_WEBHOOK_SLACK_SIG_INVALID, "Signing secret not configured", req_id);
        return false;
    };
    defer std.heap.page_allocator.free(secret);
    if (secret.len == 0) {
        log.err("slack.interactions.empty_signing_secret req_id={s}", .{req_id});
        common.errorResponse(res, ec.ERR_WEBHOOK_SLACK_SIG_INVALID, "Signing secret not configured", req_id);
        return false;
    }

    const timestamp = req.header(ec.SLACK_TS_HEADER) orelse {
        common.errorResponse(res, ec.ERR_WEBHOOK_SLACK_SIG_INVALID, "Missing timestamp header", req_id);
        return false;
    };
    const signature = req.header(ec.SLACK_SIG_HEADER) orelse {
        common.errorResponse(res, ec.ERR_WEBHOOK_SLACK_SIG_INVALID, "Missing signature header", req_id);
        return false;
    };

    if (!webhook_verify.isTimestampFresh(timestamp, ec.SLACK_MAX_TS_DRIFT_SECONDS)) {
        common.errorResponse(res, ec.ERR_WEBHOOK_SLACK_TIMESTAMP_STALE, "Timestamp too old", req_id);
        return false;
    }

    if (!webhook_verify.verifySignature(webhook_verify.SLACK, secret, timestamp, body, signature)) {
        log.warn("slack.interactions.sig_invalid req_id={s}", .{req_id});
        common.errorResponse(res, ec.ERR_WEBHOOK_SLACK_SIG_INVALID, "Invalid signature", req_id);
        return false;
    }
    return true;
}

/// Extract the `payload` field from a URL-encoded body and URL-decode it.
/// Slack sends: payload=%7B%22type%22%3A%22block_actions%22...%7D
fn extractPayloadField(alloc: std.mem.Allocator, body: []const u8) ?[]const u8 {
    const prefix = "payload=";
    const start = std.mem.indexOf(u8, body, prefix) orelse return null;
    const encoded = body[start + prefix.len ..];
    // Find end of field (& separator or end of string)
    const field = if (std.mem.indexOfScalar(u8, encoded, '&')) |amp| encoded[0..amp] else encoded;
    return urlDecode(alloc, field) catch null;
}

/// URL-decode a percent-encoded string. + → space, %XX → byte.
fn urlDecode(alloc: std.mem.Allocator, input: []const u8) ![]const u8 {
    var out: std.ArrayList(u8) = .{};
    defer out.deinit(alloc);
    var i: usize = 0;
    while (i < input.len) {
        if (input[i] == '+') {
            try out.append(alloc, ' ');
            i += 1;
        } else if (input[i] == '%' and i + 2 < input.len) {
            const high = hexVal(input[i + 1]) orelse {
                try out.append(alloc, input[i]);
                i += 1;
                continue;
            };
            const low = hexVal(input[i + 2]) orelse {
                try out.append(alloc, input[i]);
                i += 1;
                continue;
            };
            try out.append(alloc, (high << 4) | low);
            i += 3;
        } else {
            try out.append(alloc, input[i]);
            i += 1;
        }
    }
    return out.toOwnedSlice(alloc);
}

fn hexVal(c: u8) ?u8 {
    return switch (c) {
        '0'...'9' => c - '0',
        'a'...'f' => c - 'a' + 10,
        'A'...'F' => c - 'A' + 10,
        else => null,
    };
}

// ── Tests ─────────────────────────────────────────────────────────────────────

test "urlDecode: percent-encoded JSON decoded" {
    const alloc = std.testing.allocator;
    const input = "%7B%22type%22%3A%22block_actions%22%7D";
    const out = try urlDecode(alloc, input);
    defer alloc.free(out);
    try std.testing.expectEqualStrings("{\"type\":\"block_actions\"}", out);
}

test "urlDecode: plus decoded as space" {
    const alloc = std.testing.allocator;
    const out = try urlDecode(alloc, "hello+world");
    defer alloc.free(out);
    try std.testing.expectEqualStrings("hello world", out);
}

test "extractPayloadField: extracts payload from URL-encoded body" {
    const alloc = std.testing.allocator;
    const body = "payload=%7B%22type%22%3A%22block_actions%22%7D";
    const out = extractPayloadField(alloc, body);
    defer if (out) |s| alloc.free(s);
    try std.testing.expect(out != null);
    try std.testing.expectEqualStrings("{\"type\":\"block_actions\"}", out.?);
}

test "gate action_id: zombie_id parsed correctly" {
    const rest = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11_my_action_123_approve";
    const UUID_LEN = 36;
    try std.testing.expect(rest.len > UUID_LEN);
    try std.testing.expectEqualStrings("0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11", rest[0..UUID_LEN]);
    try std.testing.expectEqual(@as(u8, '_'), rest[UUID_LEN]);
}
