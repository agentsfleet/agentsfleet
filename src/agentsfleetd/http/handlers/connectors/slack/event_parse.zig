//! Pure Slack Events API body → typed event (RULE TGU). No I/O, no allocation —
//! the returned slices borrow from the caller's parsed JSON (valid until its
//! deinit), mirroring `connectors/slack/callback.zig`'s `parseSlackToken`. The
//! handler owns the `std.json.Parsed` and maps each variant to a response.

const std = @import("std");

pub const Mention = struct {
    team_id: []const u8,
    channel: []const u8,
    user: []const u8,
    text: []const u8,
    /// The mention's own stream timestamp — the dedup key + the thread anchor
    /// when there is no `thread_ts` (a top-level mention starts its own thread).
    ts: []const u8,
    thread_ts: ?[]const u8,
};

pub const ParsedEvent = union(enum) {
    /// Slack Request-URL handshake — echo `challenge` verbatim (no install yet).
    url_verification: []const u8,
    /// A member @mentioned the bot.
    app_mention: Mention,
    /// Any other body (non-mention event, unsupported type, retry wrapper we do
    /// not act on) — the handler 200-acks it as a no-op so Slack stops retrying.
    ignore,
};

const F_TYPE = "type";
const F_CHALLENGE = "challenge";
const F_TEAM_ID = "team_id";
const F_EVENT = "event";
const F_CHANNEL = "channel";
const F_USER = "user";
const F_TEXT = "text";
const F_TS = "ts";
const F_THREAD_TS = "thread_ts";
const T_URL_VERIFICATION = "url_verification";
const T_EVENT_CALLBACK = "event_callback";
const T_APP_MENTION = "app_mention";

/// Classify a parsed Slack Events API root object. A missing required field on
/// what looks like a mention degrades to `.ignore` (200-ack), never a 4xx —
/// the signature already authenticated the sender, so a shape we do not handle
/// is a no-op, not an error Slack should retry against.
pub fn parseSlackEvent(root: std.json.ObjectMap) ParsedEvent {
    const type_str = strField(root, F_TYPE) orelse return .ignore;
    if (std.mem.eql(u8, type_str, T_URL_VERIFICATION))
        return .{ .url_verification = strField(root, F_CHALLENGE) orelse return .ignore };
    if (!std.mem.eql(u8, type_str, T_EVENT_CALLBACK)) return .ignore;

    const team_id = strField(root, F_TEAM_ID) orelse return .ignore;
    const event = objField(root, F_EVENT) orelse return .ignore;
    if (!std.mem.eql(u8, strField(event, F_TYPE) orelse return .ignore, T_APP_MENTION)) return .ignore;
    return .{ .app_mention = .{
        .team_id = team_id,
        .channel = strField(event, F_CHANNEL) orelse return .ignore,
        .user = strField(event, F_USER) orelse return .ignore,
        .text = strField(event, F_TEXT) orelse "",
        .ts = strField(event, F_TS) orelse return .ignore,
        .thread_ts = strField(event, F_THREAD_TS),
    } };
}

fn strField(obj: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    return switch (obj.get(key) orelse return null) {
        .string => |s| s,
        else => null,
    };
}

fn objField(obj: std.json.ObjectMap, key: []const u8) ?std.json.ObjectMap {
    return switch (obj.get(key) orelse return null) {
        .object => |o| o,
        else => null,
    };
}

// ── Tests (Dim 2.3: url_verification echo + mention field extraction) ────────

const testing = std.testing;

fn parseRoot(body: []const u8) !std.json.Parsed(std.json.Value) {
    return std.json.parseFromSlice(std.json.Value, testing.allocator, body, .{});
}

test "parseSlackEvent: url_verification returns the challenge verbatim" {
    var p = try parseRoot("{\"type\":\"url_verification\",\"challenge\":\"3eZbrw1a\"}");
    defer p.deinit();
    switch (parseSlackEvent(p.value.object)) {
        .url_verification => |c| try testing.expectEqualStrings("3eZbrw1a", c),
        else => return error.TestExpectedEqual,
    }
}

test "parseSlackEvent: app_mention extracts team/channel/user/text/ts + thread_ts" {
    const body =
        "{\"type\":\"event_callback\",\"team_id\":\"T024BE7LH\"," ++
        "\"event\":{\"type\":\"app_mention\",\"channel\":\"C061EG9\",\"user\":\"U042\"," ++
        "\"text\":\"<@U0BOT> what's our prod called?\",\"ts\":\"1700000000.000200\",\"thread_ts\":\"1699999999.000100\"}}";
    var p = try parseRoot(body);
    defer p.deinit();
    switch (parseSlackEvent(p.value.object)) {
        .app_mention => |m| {
            try testing.expectEqualStrings("T024BE7LH", m.team_id);
            try testing.expectEqualStrings("C061EG9", m.channel);
            try testing.expectEqualStrings("U042", m.user);
            try testing.expectEqualStrings("<@U0BOT> what's our prod called?", m.text);
            try testing.expectEqualStrings("1700000000.000200", m.ts);
            try testing.expectEqualStrings("1699999999.000100", m.thread_ts.?);
        },
        else => return error.TestExpectedEqual,
    }
}

test "parseSlackEvent: a top-level mention has a null thread_ts (anchors its own thread)" {
    const body =
        "{\"type\":\"event_callback\",\"team_id\":\"T024BE7LH\"," ++
        "\"event\":{\"type\":\"app_mention\",\"channel\":\"C061EG9\",\"user\":\"U042\",\"text\":\"hi\",\"ts\":\"1700000000.000200\"}}";
    var p = try parseRoot(body);
    defer p.deinit();
    switch (parseSlackEvent(p.value.object)) {
        .app_mention => |m| try testing.expect(m.thread_ts == null),
        else => return error.TestExpectedEqual,
    }
}

test "parseSlackEvent: a non-mention inner event is ignored (200-ack no-op)" {
    const body =
        "{\"type\":\"event_callback\",\"team_id\":\"T024BE7LH\"," ++
        "\"event\":{\"type\":\"message\",\"channel\":\"C061EG9\",\"user\":\"U042\",\"text\":\"hi\",\"ts\":\"1700000000.000200\"}}";
    var p = try parseRoot(body);
    defer p.deinit();
    try testing.expect(parseSlackEvent(p.value.object) == .ignore);
}

test "parseSlackEvent: an unknown top-level type is ignored" {
    var p = try parseRoot("{\"type\":\"something_else\"}");
    defer p.deinit();
    try testing.expect(parseSlackEvent(p.value.object) == .ignore);
}
