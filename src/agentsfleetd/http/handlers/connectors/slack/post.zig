//! §4 — post the channel bot's answer back to Slack via `chat.postMessage`.
//!
//! Called ONLY by the `connector:outbound` worker's slack dispatch arm (never on
//! the runner's report path, and never by the sandboxed runner itself — it holds
//! no bot token). Resolves the per-install bot token from the
//! `(workspace_id,'fleet:slack')` vault handle, reads the originating channel +
//! reply thread from the mention event's `request_json`, and posts the answer
//! threaded. Best-effort: every failure is logged `UZ-SLK-030` and returns a
//! verdict the worker uses for bounded backoff — the run itself never fails.

const std = @import("std");
const pg = @import("pg");
const logging = @import("log");
const ec = @import("../../../../errors/error_registry.zig");
const vault = @import("../../../../state/vault.zig");
const credential_key = @import("../../../../fleet_runtime/credential_key.zig");
const PgQuery = @import("../../../../db/pg_query.zig").PgQuery;
const spec = @import("spec.zig");

const log = logging.scoped(.connector_slack);

/// Slack Web API root. Overridable to a loopback FakeSlack in tests via
/// `Context.connector_slack_api_base_override` (the worker passes it through).
pub const SLACK_API_BASE_DEFAULT = "https://slack.com/api";
const CHAT_POSTMESSAGE = "/chat.postMessage";
const HTTP_OK: u16 = 200;
const HTTP_TOO_MANY_REQUESTS: u16 = 429;
const HTTP_SERVER_ERROR_FLOOR: u16 = 500;
const F_BOT_TOKEN = "bot_token";
// request_json fields (mirror events.zig buildRequestJson).
const RQ_CHANNEL = "channel_id";
const RQ_THREAD = "reply_thread_ts";
const F_OK = "ok";

/// Delivery verdict for the worker's retry policy. `retryable` → back off + retry
/// (rate-limit / 5xx / transport); `permanent` → give up (bad token, app error
/// like `channel_not_found`/`missing_scope`, unpostable event) — retry won't help.
pub const Outcome = enum { delivered, retryable, permanent };

const SELECT_EVENT_REQUEST_JSON =
    \\SELECT request_json::text FROM core.fleet_events WHERE fleet_id = $1::uuid AND event_id = $2
;

/// Post `answer` to the Slack thread that mention `event_id` arrived on. Resolves
/// the channel + reply thread from the event's `request_json` and the bot token
/// from the workspace's `fleet:slack` vault handle. `api_base` is the Slack Web
/// API root. Never throws — returns a verdict; all failures log `UZ-SLK-030`.
pub fn deliver(
    alloc: std.mem.Allocator,
    io: std.Io,
    pool: *pg.Pool,
    api_base: []const u8,
    workspace_id: []const u8,
    fleet_id: []const u8,
    event_id: []const u8,
    answer: []const u8,
) Outcome {
    const conn = pool.acquire() catch return .retryable; // DB blip → retry
    defer pool.release(conn);

    // Channel + reply thread live in the event's request_json (what events.zig
    // XADDed). A missing/garbled row is permanent — there is nowhere to post.
    var req = loadRequestJson(alloc, conn, fleet_id, event_id) catch |err| {
        log.warn("slack_post_event_load_failed", .{ .error_code = ec.ERR_SLACK_OUTBOUND_POST_FAILED, .fleet_id = fleet_id, .err = @errorName(err) });
        return .permanent;
    } orelse return .permanent;
    defer req.deinit();
    const robj = switch (req.value) {
        .object => |o| o,
        else => return .permanent,
    };
    const channel = strField(robj, RQ_CHANNEL) orelse return .permanent;
    const thread_ts = strField(robj, RQ_THREAD) orelse return .permanent;

    const token = loadBotToken(alloc, conn, workspace_id) catch |err| {
        // No/garbled handle → uninstalled or misconfigured; a retry won't help.
        log.warn("slack_post_token_load_failed", .{ .error_code = ec.ERR_SLACK_OUTBOUND_POST_FAILED, .workspace_id = workspace_id, .err = @errorName(err) });
        return .permanent;
    };
    defer alloc.free(token);

    return postMessage(alloc, io, api_base, token, channel, thread_ts, answer);
}

/// SELECT the event row's `request_json` and parse it (owned Parsed — std.json
/// copies strings into its arena, so it outlives the row/`q.deinit`). Null when
/// the event row is gone.
fn loadRequestJson(alloc: std.mem.Allocator, conn: *pg.Conn, fleet_id: []const u8, event_id: []const u8) !?std.json.Parsed(std.json.Value) {
    var q = PgQuery.from(try conn.query(SELECT_EVENT_REQUEST_JSON, .{ fleet_id, event_id }));
    defer q.deinit();
    const row = try q.next() orelse return null;
    const json = try row.get([]const u8, 0);
    return try std.json.parseFromSlice(std.json.Value, alloc, json, .{});
}

/// Resolve the per-install bot token from the `(workspace_id,'fleet:slack')`
/// vault handle callback.zig wrote (RULE VLT). Caller owns the returned token.
fn loadBotToken(alloc: std.mem.Allocator, conn: *pg.Conn, workspace_id: []const u8) ![]const u8 {
    const key = try credential_key.allocKeyName(alloc, spec.PROVIDER); // "fleet:slack"
    defer alloc.free(key);
    var parsed = try vault.loadJson(alloc, conn, workspace_id, key);
    defer parsed.deinit();
    const obj = parsed.value.object; // loadJson guarantees `.object`
    const tok = strField(obj, F_BOT_TOKEN) orelse return error.SlackBotTokenMissing;
    return alloc.dupe(u8, tok);
}

/// POST `{channel, thread_ts, text}` to `chat.postMessage` with the bot token.
/// Slack returns 200 with `{ok:true|false}` for app-level outcomes, 429 for
/// rate-limit, 5xx for its own faults — mapped to the retry verdict.
fn postMessage(
    alloc: std.mem.Allocator,
    io: std.Io,
    api_base: []const u8,
    token: []const u8,
    channel: []const u8,
    thread_ts: []const u8,
    text: []const u8,
) Outcome {
    const url = std.fmt.allocPrint(alloc, "{s}{s}", .{ api_base, CHAT_POSTMESSAGE }) catch return .retryable;
    defer alloc.free(url);
    // std.json escapes `text` (arbitrary model output) safely.
    const body = std.json.Stringify.valueAlloc(alloc, .{ .channel = channel, .thread_ts = thread_ts, .text = text }, .{}) catch return .retryable;
    defer alloc.free(body);
    const auth = std.fmt.allocPrint(alloc, "Bearer {s}", .{token}) catch return .retryable;
    defer alloc.free(auth);

    var client: std.http.Client = .{ .allocator = alloc, .io = io };
    defer client.deinit();
    var resp_body: std.ArrayList(u8) = .empty;
    var aw: std.Io.Writer.Allocating = .fromArrayList(alloc, &resp_body);
    const headers = [_]std.http.Header{
        .{ .name = "authorization", .value = auth },
        .{ .name = "content-type", .value = "application/json; charset=utf-8" },
    };
    const result = client.fetch(.{
        .location = .{ .url = url },
        .method = .POST,
        .payload = body,
        .extra_headers = &headers,
        .response_writer = &aw.writer,
    }) catch |err| {
        aw.deinit();
        log.warn("slack_post_transport_failed", .{ .error_code = ec.ERR_SLACK_OUTBOUND_POST_FAILED, .err = @errorName(err) });
        return .retryable; // transport / dial error → retry
    };
    const status = @intFromEnum(result.status);
    // Read the body via the writer (the seed ArrayList is stale once it grows).
    const resp = aw.toOwnedSlice() catch {
        aw.deinit();
        return classifyStatus(status);
    };
    defer alloc.free(resp);
    return classify(alloc, status, resp);
}

/// Map HTTP status + Slack body to a verdict. 200 with `ok:true` is the only
/// success; a 200 `ok:false` is an app error (permanent — retry won't fix a bad
/// scope/channel); 429 + 5xx are retryable.
fn classify(alloc: std.mem.Allocator, status: u16, resp: []const u8) Outcome {
    if (status == HTTP_TOO_MANY_REQUESTS or status >= HTTP_SERVER_ERROR_FLOOR) {
        log.warn("slack_post_retryable", .{ .error_code = ec.ERR_SLACK_OUTBOUND_POST_FAILED, .status = status });
        return .retryable;
    }
    if (status != HTTP_OK) {
        log.warn("slack_post_unexpected_status", .{ .error_code = ec.ERR_SLACK_OUTBOUND_POST_FAILED, .status = status });
        return .permanent;
    }
    if (slackOk(alloc, resp)) return .delivered;
    log.warn("slack_post_app_error", .{ .error_code = ec.ERR_SLACK_OUTBOUND_POST_FAILED, .status = status });
    return .permanent;
}

/// Verdict when the body could not be read (rare post-200): trust the status.
fn classifyStatus(status: u16) Outcome {
    if (status == HTTP_TOO_MANY_REQUESTS or status >= HTTP_SERVER_ERROR_FLOOR) return .retryable;
    if (status == HTTP_OK) return .delivered; // 200 with an unread body — assume ok
    return .permanent;
}

/// True iff the Slack response JSON has `"ok": true`. A parse failure is treated
/// as not-ok (permanent) — a 200 that is not valid Slack JSON is anomalous.
fn slackOk(alloc: std.mem.Allocator, resp: []const u8) bool {
    var parsed = std.json.parseFromSlice(std.json.Value, alloc, resp, .{}) catch return false;
    defer parsed.deinit();
    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return false,
    };
    const v = obj.get(F_OK) orelse return false;
    return v == .bool and v.bool;
}

fn strField(obj: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    const v = obj.get(key) orelse return null;
    return switch (v) {
        .string => |s| s,
        else => null,
    };
}

// ── Tests (pure classification; the DB + HTTP round-trip is integration-gated) ──

const testing = std.testing;

test "classify: 200 ok:true is delivered" {
    try testing.expectEqual(Outcome.delivered, classify(testing.allocator, 200, "{\"ok\":true,\"ts\":\"1700000000.000100\"}"));
}

test "classify: 200 ok:false is a permanent app error" {
    try testing.expectEqual(Outcome.permanent, classify(testing.allocator, 200, "{\"ok\":false,\"error\":\"channel_not_found\"}"));
}

test "classify: 429 + 5xx are retryable, other 4xx permanent" {
    try testing.expectEqual(Outcome.retryable, classify(testing.allocator, 429, ""));
    try testing.expectEqual(Outcome.retryable, classify(testing.allocator, 503, ""));
    try testing.expectEqual(Outcome.permanent, classify(testing.allocator, 404, ""));
}

test "slackOk: true only for {ok:true}" {
    try testing.expect(slackOk(testing.allocator, "{\"ok\":true}"));
    try testing.expect(!slackOk(testing.allocator, "{\"ok\":false}"));
    try testing.expect(!slackOk(testing.allocator, "not json"));
}
