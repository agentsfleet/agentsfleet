//! M9_001 §4.0 — Grant request notification fan-out.
//! Stores Slack Block Kit payload in Redis for the notification provider (M8 plugin).
//! Always logs a dashboard notification via activity_stream regardless of provider.
//!
//! Redis key: grant:notify:{zombie_id}:{grant_id}
//! TTL: 7200s (2 hours — same as gate pending TTL)
//! Consumer: M8 Slack/Discord plugin reads and POSTs to workspace channels.

const std = @import("std");
const pg = @import("pg");
const queue_redis = @import("../../queue/redis.zig");
const activity_stream = @import("../activity_stream.zig");
const approval_slack = @import("../approval_gate_slack.zig");

const log = std.log.scoped(.grant_notifier);

pub const GRANT_NOTIFY_TTL_SECONDS: u32 = 7200;
const GRANT_NOTIFY_KEY_PREFIX = "grant:notify:";
const GRANT_NONCE_KEY_PREFIX = "grant:nonce:";

// ── Nonce generation ───────────────────────────────────────────────────────

/// Generate a 32-char lowercase hex nonce from 16 random bytes. Stack-allocated.
fn generateNonce() [32]u8 {
    var raw: [16]u8 = undefined;
    std.crypto.random.bytes(&raw);
    return std.fmt.bytesToHex(raw, .lower);
}

/// Store nonce at grant:nonce:{grant_id} in Redis for later webhook verification.
fn storeNonceToRedis(
    queue: *queue_redis.Client,
    grant_id: []const u8,
    nonce: []const u8,
) void {
    var key_buf: [256]u8 = undefined;
    const key = std.fmt.bufPrint(&key_buf, "{s}{s}", .{
        GRANT_NONCE_KEY_PREFIX, grant_id,
    }) catch return;
    queue.setEx(key, nonce, GRANT_NOTIFY_TTL_SECONDS) catch |err| {
        log.warn("grant_notifier.nonce_fail grant_id={s} err={s}", .{
            grant_id, @errorName(err),
        });
    };
}

// ── Payload builder ────────────────────────────────────────────────────────

/// Build a Slack Block Kit JSON payload for a grant request.
/// Button values are encoded as "{grant_id}|{nonce}" so the approval webhook
/// can verify the nonce before accepting the decision. Caller owns the returned slice.
fn buildGrantSlackMessage(
    alloc: std.mem.Allocator,
    zombie_name: []const u8,
    service: []const u8,
    reason: []const u8,
    grant_id: []const u8,
    nonce: []const u8,
) ![]const u8 {
    var buf: std.ArrayList(u8) = .{};
    errdefer buf.deinit(alloc);
    const w = buf.writer(alloc);

    const header = try std.fmt.allocPrint(alloc,
        "\u{1F510} *{s}* is requesting *{s}* access", .{ zombie_name, service });
    defer alloc.free(header);
    const reason_text = try std.fmt.allocPrint(alloc, "Reason: \"{s}\"\n\nScopes: Full access (*)", .{reason});
    defer alloc.free(reason_text);

    try w.writeAll("{\"blocks\":[");
    // Header block
    try w.writeAll("{\"type\":\"header\",\"text\":{\"type\":\"plain_text\",\"text\":\"");
    try approval_slack.writeJsonEscaped(w, header);
    try w.writeAll("\"}},");
    // Reason/scope section
    try w.writeAll("{\"type\":\"section\",\"text\":{\"type\":\"mrkdwn\",\"text\":\"");
    try approval_slack.writeJsonEscaped(w, reason_text);
    try w.writeAll("\"}},");
    // Approve / Deny actions
    try w.writeAll("{\"type\":\"actions\",\"block_id\":\"grant_");
    try approval_slack.writeJsonEscaped(w, grant_id);
    try w.writeAll("\",\"elements\":[");
    // Button value: "{grant_id}|{nonce}" — the approval webhook verifies the nonce.
    const button_value = try std.fmt.allocPrint(alloc, "{s}|{s}", .{ grant_id, nonce });
    defer alloc.free(button_value);

    try w.writeAll("{\"type\":\"button\",\"text\":{\"type\":\"plain_text\",\"text\":\"Approve\"}");
    try w.writeAll(",\"style\":\"primary\",\"action_id\":\"grant_approve\",\"value\":\"");
    try approval_slack.writeJsonEscaped(w, button_value);
    try w.writeAll("\"},{\"type\":\"button\",\"text\":{\"type\":\"plain_text\",\"text\":\"Deny\"}");
    try w.writeAll(",\"style\":\"danger\",\"action_id\":\"grant_deny\",\"value\":\"");
    try approval_slack.writeJsonEscaped(w, button_value);
    try w.writeAll("\"}]}],\"text\":\"");
    try approval_slack.writeJsonEscaped(w, header);
    try w.writeAll("\"}");

    return buf.toOwnedSlice(alloc);
}

// ── Redis store ────────────────────────────────────────────────────────────

fn storeToRedis(
    queue: *queue_redis.Client,
    zombie_id: []const u8,
    grant_id: []const u8,
    payload: []const u8,
) void {
    var key_buf: [256]u8 = undefined;
    const key = std.fmt.bufPrint(&key_buf, "{s}{s}:{s}", .{
        GRANT_NOTIFY_KEY_PREFIX, zombie_id, grant_id,
    }) catch return;
    queue.setEx(key, payload, GRANT_NOTIFY_TTL_SECONDS) catch |err| {
        log.warn("grant_notifier.redis_fail zombie_id={s} grant_id={s} err={s}", .{
            zombie_id, grant_id, @errorName(err),
        });
    };
}

// ── Dashboard fallback ─────────────────────────────────────────────────────

fn logDashboard(
    pool: *pg.Pool,
    alloc: std.mem.Allocator,
    zombie_id: []const u8,
    workspace_id: []const u8,
    grant_id: []const u8,
    service: []const u8,
) void {
    const detail = std.fmt.allocPrint(alloc, "grant_request:{s}:{s}", .{ service, grant_id }) catch grant_id;
    activity_stream.logEvent(pool, alloc, .{
        .zombie_id    = zombie_id,
        .workspace_id = workspace_id,
        .event_type   = "grant.requested",
        .detail       = detail,
    });
}

// ── Public API ─────────────────────────────────────────────────────────────

/// Fan out grant request notifications.
/// (1) Generates a one-time nonce stored at grant:nonce:{grant_id} in Redis.
/// (2) Builds Slack Block Kit JSON (with nonce in button values) and stores to Redis.
/// (3) Always logs a dashboard activity_stream event as fallback / audit trail.
pub fn notifyGrantRequest(
    pool: *pg.Pool,
    queue: *queue_redis.Client,
    alloc: std.mem.Allocator,
    zombie_id: []const u8,
    workspace_id: []const u8,
    grant_id: []const u8,
    zombie_name: []const u8,
    service: []const u8,
    reason: []const u8,
) void {
    const nonce_arr = generateNonce();
    const nonce: []const u8 = nonce_arr[0..];
    storeNonceToRedis(queue, grant_id, nonce);

    const slack_msg = buildGrantSlackMessage(alloc, zombie_name, service, reason, grant_id, nonce) catch |err| {
        log.warn("grant_notifier.build_fail err={s}", .{@errorName(err)});
        logDashboard(pool, alloc, zombie_id, workspace_id, grant_id, service);
        return;
    };
    defer alloc.free(slack_msg);

    storeToRedis(queue, zombie_id, grant_id, slack_msg);
    logDashboard(pool, alloc, zombie_id, workspace_id, grant_id, service);

    log.info("grant_notifier.sent zombie_id={s} service={s} grant_id={s}", .{
        zombie_id, service, grant_id,
    });
}

// ── Tests ──────────────────────────────────────────────────────────────────

test "buildGrantSlackMessage: produces valid JSON with grant_id and nonce in button values" {
    const alloc = std.testing.allocator;
    const nonce = "abcd1234abcd1234abcd1234abcd1234";
    const msg = try buildGrantSlackMessage(alloc, "my-zombie", "slack", "Need to post to #hiring", "uzg_001", nonce);
    defer alloc.free(msg);
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, msg, .{});
    defer parsed.deinit();
    try std.testing.expect(parsed.value == .object);
    try std.testing.expect(std.mem.indexOf(u8, msg, "uzg_001") != null);
    try std.testing.expect(std.mem.indexOf(u8, msg, nonce) != null);
    try std.testing.expect(std.mem.indexOf(u8, msg, "Approve") != null);
    try std.testing.expect(std.mem.indexOf(u8, msg, "Deny") != null);
}

test "buildGrantSlackMessage: escapes JSON-unsafe characters in reason" {
    const alloc = std.testing.allocator;
    const msg = try buildGrantSlackMessage(alloc, "z", "slack", "reason \"quoted\"", "g1", "nonce1234567890123456789012345678");
    defer alloc.free(msg);
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, msg, .{});
    defer parsed.deinit();
    try std.testing.expect(parsed.value == .object);
}

test "buildGrantSlackMessage: escapes backslash in zombie_name" {
    const alloc = std.testing.allocator;
    const msg = try buildGrantSlackMessage(alloc, "z\\n", "discord", "reason", "g2", "nonce1234567890123456789012345678");
    defer alloc.free(msg);
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, msg, .{});
    defer parsed.deinit();
    try std.testing.expect(parsed.value == .object);
}

test "generateNonce: produces 32-char hex string" {
    const n = generateNonce();
    try std.testing.expect(n.len == 32);
    for (n) |c| {
        try std.testing.expect((c >= '0' and c <= '9') or (c >= 'a' and c <= 'f'));
    }
}
