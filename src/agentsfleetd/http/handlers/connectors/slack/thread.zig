//! §4 E — best-effort recent-thread re-read at mention ingress.
//!
//! The bot is mention-only and blind to intervening non-mention messages, so
//! same-thread continuity requires re-reading the thread on each mention (spec
//! §4). We do it at INGRESS (Indy-acked placement): `events.zig` calls
//! `fetchRecent` before building the event's `request_json`, so the bounded
//! last-N thread lands in the payload the runner later leases.
//!
//! Everything here is BEST-EFFORT: any failure (no token, transport, non-200,
//! unparseable body) degrades to an EMPTY thread — the answer still works from
//! durable channel memory + the mention text, and dedup on `event.ts` makes a
//! slow Slack call safe to retry. There is no per-call timeout primitive on
//! `std.http.Client.fetch` in Zig 0.16 (neither `oauth2.exchange` nor `post.zig`
//! has one); the call is bounded structurally by `RECENT_THREAD_LIMIT` (a small,
//! fast `conversations.replies` page) rather than a bespoke watchdog thread.

const std = @import("std");
const pg = @import("pg");
const logging = @import("log");

const post = @import("post.zig");

const log = logging.scoped(.connector_slack);

/// Max thread messages pulled per mention — bounds the ingress fetch latency +
/// the `request_json` size. "Recent thread" is a reasoning aid, not the record.
pub const RECENT_THREAD_LIMIT: usize = 20;

const CONVERSATIONS_REPLIES = "/conversations.replies";
const HTTP_OK: u16 = 200;
// conversations.replies message fields we keep (attribution + dedup + content).
const F_MESSAGES = "messages";
const F_OK = "ok";
const F_USER = "user";
const F_TS = "ts";
const F_TEXT = "text";

/// One thread message, arena-owned. `user`/`ts` may be empty (a bot/system
/// message can omit them); `text` is always present (empty-text messages are
/// skipped at parse time — they carry nothing for the model).
pub const Msg = struct {
    user: []const u8,
    ts: []const u8,
    text: []const u8,
};

/// Owned result of a thread re-read. `msgs` (possibly empty) is valid until
/// `deinit`; a single arena frees the whole bounded collection.
pub const Recent = struct {
    arena: std.heap.ArenaAllocator,
    msgs: []const Msg,

    pub fn deinit(self: *Recent) void {
        self.arena.deinit();
    }
};

/// Fetch the last-N messages of thread `thread_ts` in `channel` via Slack
/// `conversations.replies`, authed with the workspace's bot token. NEVER throws —
/// any failure returns an empty `Recent`. `api_base` is the Slack Web API root
/// (overridden to a loopback FakeSlack in tests). Caller owns the result
/// (`.deinit()`).
pub fn fetchRecent(
    alloc: std.mem.Allocator,
    io: std.Io,
    conn: *pg.Conn,
    api_base: []const u8,
    workspace_id: []const u8,
    channel: []const u8,
    thread_ts: []const u8,
) Recent {
    var arena = std.heap.ArenaAllocator.init(alloc);
    // `arena` is moved into the returned Recent on both paths — the scratch
    // allocator (`alloc`) holds the transient HTTP body + parse; only the kept
    // Msg strings are duped into the arena.
    const msgs = fetchInto(arena.allocator(), alloc, io, conn, api_base, workspace_id, channel, thread_ts) catch |err| {
        log.debug("slack_thread_refetch_degraded", .{ .workspace_id = workspace_id, .err = @errorName(err) });
        return .{ .arena = arena, .msgs = &.{} };
    };
    return .{ .arena = arena, .msgs = msgs };
}

/// GET `conversations.replies`, parse, and dupe the kept messages into `arena`.
/// `scratch` backs the transient HTTP client, response slice, and JSON parse
/// (freed before returning); only the returned `Msg` slices live in `arena`.
fn fetchInto(
    arena: std.mem.Allocator,
    scratch: std.mem.Allocator,
    io: std.Io,
    conn: *pg.Conn,
    api_base: []const u8,
    workspace_id: []const u8,
    channel: []const u8,
    thread_ts: []const u8,
) ![]const Msg {
    const token = try post.loadBotToken(scratch, conn, workspace_id);
    defer scratch.free(token);

    // channel + thread_ts are Slack-issued opaque ids (`[A-Z0-9.]`) — no query
    // escaping needed. limit caps the page (spec: bounded last-N).
    const url = try std.fmt.allocPrint(scratch, "{s}{s}?channel={s}&ts={s}&limit={d}", .{ api_base, CONVERSATIONS_REPLIES, channel, thread_ts, RECENT_THREAD_LIMIT });
    defer scratch.free(url);
    const auth = try std.fmt.allocPrint(scratch, "Bearer {s}", .{token});
    defer scratch.free(auth);

    var client: std.http.Client = .{ .allocator = scratch, .io = io };
    defer client.deinit();
    var resp_body: std.ArrayList(u8) = .empty;
    var aw: std.Io.Writer.Allocating = .fromArrayList(scratch, &resp_body);
    errdefer aw.deinit();
    const headers = [_]std.http.Header{
        .{ .name = "authorization", .value = auth },
    };
    const result = try client.fetch(.{
        .location = .{ .url = url },
        .method = .GET,
        .extra_headers = &headers,
        .response_writer = &aw.writer,
    });
    if (@intFromEnum(result.status) != HTTP_OK) {
        aw.deinit();
        return error.SlackRepliesStatus;
    }
    // Read the body via the writer (the seed ArrayList is stale once it grows).
    const resp = try aw.toOwnedSlice();
    defer scratch.free(resp);

    return parseReplies(arena, scratch, resp);
}

/// Parse a `conversations.replies` body into arena-owned `Msg`s. `ok:false` or a
/// missing `messages` array is an error (→ empty thread upstream). Skips any
/// message lacking a non-empty `text`.
fn parseReplies(arena: std.mem.Allocator, scratch: std.mem.Allocator, resp: []const u8) ![]const Msg {
    var parsed = try std.json.parseFromSlice(std.json.Value, scratch, resp, .{});
    defer parsed.deinit();
    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return error.SlackRepliesShape,
    };
    const ok = obj.get(F_OK) orelse return error.SlackRepliesShape;
    if (ok != .bool or !ok.bool) return error.SlackRepliesNotOk;
    const messages = switch (obj.get(F_MESSAGES) orelse return error.SlackRepliesShape) {
        .array => |arr| arr,
        else => return error.SlackRepliesShape,
    };

    var list: std.ArrayList(Msg) = .empty;
    for (messages.items) |mv| {
        const mo = switch (mv) {
            .object => |o| o,
            else => continue,
        };
        const text = strField(mo, F_TEXT) orelse continue;
        if (text.len == 0) continue;
        try list.append(arena, .{
            .user = try arena.dupe(u8, strField(mo, F_USER) orelse ""),
            .ts = try arena.dupe(u8, strField(mo, F_TS) orelse ""),
            .text = try arena.dupe(u8, text),
        });
    }
    return list.toOwnedSlice(arena);
}

fn strField(obj: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    return switch (obj.get(key) orelse return null) {
        .string => |s| s,
        else => null,
    };
}

// ── Tests (pure parse; the DB + HTTP round-trip is integration-gated) ─────────

const testing = std.testing;

test "parseReplies: extracts user/ts/text, skips empty-text + non-object entries" {
    const body =
        \\{"ok":true,"messages":[
        \\  {"type":"message","user":"U1","ts":"1700000000.000100","text":"prod is called aurora"},
        \\  {"type":"message","user":"U2","ts":"1700000000.000200","text":""},
        \\  "not-an-object",
        \\  {"type":"message","user":"U3","ts":"1700000000.000300","text":"thanks"}
        \\]}
    ;
    var parsed = try parseFromSliceOwned(body);
    defer parsed.deinit();
    try testing.expectEqual(@as(usize, 2), parsed.msgs.len);
    try testing.expectEqualStrings("U1", parsed.msgs[0].user);
    try testing.expectEqualStrings("1700000000.000100", parsed.msgs[0].ts);
    try testing.expectEqualStrings("prod is called aurora", parsed.msgs[0].text);
    try testing.expectEqualStrings("thanks", parsed.msgs[1].text);
}

test "parseReplies: ok:false is an error (degrades to empty upstream)" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    try testing.expectError(error.SlackRepliesNotOk, parseReplies(arena.allocator(), testing.allocator, "{\"ok\":false,\"error\":\"thread_not_found\"}"));
}

test "parseReplies: missing messages array is a shape error" {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    defer arena.deinit();
    try testing.expectError(error.SlackRepliesShape, parseReplies(arena.allocator(), testing.allocator, "{\"ok\":true}"));
}

test "parseReplies: a message may omit user/ts (bot/system) yet keep its text" {
    var parsed = try parseFromSliceOwned("{\"ok\":true,\"messages\":[{\"type\":\"message\",\"text\":\"joined\"}]}");
    defer parsed.deinit();
    try testing.expectEqual(@as(usize, 1), parsed.msgs.len);
    try testing.expectEqualStrings("", parsed.msgs[0].user);
    try testing.expectEqualStrings("joined", parsed.msgs[0].text);
}

/// Test helper: parse a body into an owned `Recent` (arena + msgs), mirroring the
/// fetch path's ownership so tests exercise the same free discipline.
fn parseFromSliceOwned(body: []const u8) !Recent {
    var arena = std.heap.ArenaAllocator.init(testing.allocator);
    const msgs = parseReplies(arena.allocator(), testing.allocator, body) catch |err| {
        arena.deinit();
        return err;
    };
    return .{ .arena = arena, .msgs = msgs };
}
