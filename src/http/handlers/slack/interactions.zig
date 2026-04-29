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
const approval_gate = @import("../../../zombie/approval_gate.zig");
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");

const log = std.log.scoped(.http_slack_interactions);

pub const Context = common.Context;
const Hx = hx_mod.Hx;

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

pub fn innerInteraction(hx: Hx, req: *httpz.Request) void {
    const raw_body = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "Empty body");
        return;
    };

    // Slack signature verified by slack_signature middleware before this handler runs.

    // Slack sends interactions as URL-encoded: payload={url_encoded_json}
    const payload_json = extractPayloadField(hx.alloc, raw_body) orelse {
        log.warn("slack.interactions.no_payload req_id={s}", .{hx.req_id});
        hx.fail(ec.ERR_INVALID_REQUEST, "Missing payload field");
        return;
    };

    const parsed = std.json.parseFromSlice(SlackInteractionPayload, hx.alloc, payload_json, .{ .ignore_unknown_fields = true }) catch {
        log.warn("slack.interactions.parse_fail req_id={s}", .{hx.req_id});
        hx.fail(ec.ERR_INVALID_REQUEST, "Invalid interaction payload");
        return;
    };
    defer parsed.deinit();

    const actions = parsed.value.actions orelse {
        log.warn("slack.interactions.no_actions req_id={s}", .{hx.req_id});
        hx.res.status = 200;
        hx.res.body = "{}";
        return;
    };
    if (actions.len == 0) {
        hx.res.status = 200;
        hx.res.body = "{}";
        return;
    }

    const action_id = actions[0].action_id;

    if (std.mem.startsWith(u8, action_id, GATE_PREFIX)) {
        handleGateAction(hx, action_id[GATE_PREFIX.len..]);
    } else if (std.mem.startsWith(u8, action_id, GRANT_PREFIX)) {
        // Integration grant approval — M9 territory. Log and ack for now.
        log.info("slack.interactions.grant_action action_id={s} req_id={s}", .{ action_id, hx.req_id });
        hx.res.status = 200;
        hx.res.body = "{}";
    } else {
        log.warn("slack.interactions.unknown_action action_id={s} req_id={s}", .{ action_id, hx.req_id });
        hx.res.status = 200;
        hx.res.body = "{}";
    }
}

// ── Internal ─────────────────────────────────────────────────────────────────

/// Route gate_{zombie_id}_{inner_action_id}_{decision} to approval gate.
fn handleGateAction(hx: Hx, rest: []const u8) void {
    // rest = {zombie_id}_{inner_action_id}_{decision}
    // zombie_id is a UUID v7 (36 chars). Find by fixed length.
    const UUID_LEN = 36;
    if (rest.len < UUID_LEN + 2) {
        log.warn("slack.interactions.gate_short action rest={s} req_id={s}", .{ rest, hx.req_id });
        hx.res.status = 200;
        hx.res.body = "{}";
        return;
    }
    // zombie_id ends at position UUID_LEN, must be followed by '_'
    if (rest[UUID_LEN] != '_') {
        log.warn("slack.interactions.gate_bad_sep req_id={s}", .{hx.req_id});
        hx.res.status = 200;
        hx.res.body = "{}";
        return;
    }
    const zombie_id = rest[0..UUID_LEN];
    const after_zombie = rest[UUID_LEN + 1 ..];

    // Find decision at the end: last segment after final '_'
    const last_underscore = std.mem.lastIndexOfScalar(u8, after_zombie, '_') orelse {
        log.warn("slack.interactions.gate_no_decision req_id={s}", .{hx.req_id});
        hx.res.status = 200;
        hx.res.body = "{}";
        return;
    };
    const inner_action_id = after_zombie[0..last_underscore];
    const decision = after_zombie[last_underscore + 1 ..];

    if (!std.mem.eql(u8, decision, "approve") and !std.mem.eql(u8, decision, "deny")) {
        log.warn("slack.interactions.invalid_decision decision={s} req_id={s}", .{ decision, hx.req_id });
        hx.res.status = 200;
        hx.res.body = "{}";
        return;
    }

    // Funnel through the channel-agnostic resolve core. Dedup against the
    // dashboard handler and the sweeper falls out of the DB UPDATE
    // precondition WHERE status='pending'.
    const gate_status: approval_gate.GateStatus = if (std.mem.eql(u8, decision, "approve"))
        .approved
    else
        .denied;

    var outcome = approval_gate.resolve(
        hx.ctx.pool,
        hx.ctx.queue,
        hx.alloc,
        inner_action_id,
        gate_status,
        "slack:interaction",
        "",
    ) catch |err| {
        log.err("slack.interactions.resolve_fail err={s} req_id={s}", .{ @errorName(err), hx.req_id });
        hx.res.status = 200;
        hx.res.body = "{}";
        return;
    };
    defer switch (outcome) {
        .resolved => |*r| @constCast(r).deinit(hx.alloc),
        .already_resolved => |*r| @constCast(r).deinit(hx.alloc),
        .not_found => {},
    };

    switch (outcome) {
        .not_found => {
            log.debug("slack.interactions.gate_not_found action_id={s} req_id={s}", .{ inner_action_id, hx.req_id });
            hx.res.status = 200;
            hx.res.body = "{\"error\":\"gate_expired\"}";
            return;
        },
        .resolved => log.info("slack.interactions.gate_resolved zombie_id={s} action_id={s} decision={s} req_id={s}", .{
            zombie_id, inner_action_id, decision, hx.req_id,
        }),
        .already_resolved => |r| log.info("slack.interactions.gate_already_resolved zombie_id={s} action_id={s} prior_outcome={s} prior_by={s} req_id={s}", .{
            zombie_id, inner_action_id, r.outcome.toSlice(), r.resolved_by, hx.req_id,
        }),
    }
    hx.res.status = 200;
    hx.res.body = "{}";
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

test "urlDecode: empty string returns empty" {
    const alloc = std.testing.allocator;
    const out = try urlDecode(alloc, "");
    defer alloc.free(out);
    try std.testing.expectEqualStrings("", out);
}

test "urlDecode: plain string passes through unchanged" {
    const alloc = std.testing.allocator;
    const out = try urlDecode(alloc, "helloworld");
    defer alloc.free(out);
    try std.testing.expectEqualStrings("helloworld", out);
}

test "urlDecode: lowercase hex digits decoded correctly" {
    // Slack sends lowercase percent-encoding (%7b, not %7B).
    const alloc = std.testing.allocator;
    const out = try urlDecode(alloc, "%7b%22key%22%7d");
    defer alloc.free(out);
    try std.testing.expectEqualStrings("{\"key\"}", out);
}

test "urlDecode: malformed percent sequences pass through literally" {
    // Truncated %XX (0 or 1 trailing chars): '%' emitted as literal, no crash.
    const alloc = std.testing.allocator;
    const out1 = try urlDecode(alloc, "abc%");
    defer alloc.free(out1);
    try std.testing.expectEqualStrings("abc%", out1);
    const out2 = try urlDecode(alloc, "a%2");
    defer alloc.free(out2);
    try std.testing.expectEqualStrings("a%2", out2);
}

test "extractPayloadField: missing payload field or empty body returns null" {
    const alloc = std.testing.allocator;
    try std.testing.expect(extractPayloadField(alloc, "") == null);
    try std.testing.expect(extractPayloadField(alloc, "type=block_actions&other=foo") == null);
}

test "extractPayloadField: payload value stops at ampersand" {
    const alloc = std.testing.allocator;
    const body = "payload=%7B%22type%22%3A%22block_actions%22%7D&extra=ignored";
    const out = extractPayloadField(alloc, body);
    defer if (out) |s| alloc.free(s);
    try std.testing.expect(out != null);
    try std.testing.expectEqualStrings("{\"type\":\"block_actions\"}", out.?);
}
