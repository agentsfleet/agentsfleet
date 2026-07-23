//! Post the channel bot's answer back to Slack via `chat.postMessage`.
//!
//! Called ONLY by the `connector:outbound` worker's slack dispatch arm (never on
//! the runner's report path, and never by the sandboxed runner itself — it holds
//! no bot token). Resolves the per-install bot token from the
//! `(workspace_id,'fleet:slack')` vault handle, reads the originating channel +
//! reply thread from the mention event's `request_json`, RELEASES the pooled
//! conn, and only then posts the answer threaded (deadline-armed — a stalled
//! vendor can never park a pool slot or the worker). Best-effort: every failure
//! is logged `UZ-SLK-030` and returns a verdict the worker uses for bounded
//! backoff — the run itself never fails.

const std = @import("std");
const pg = @import("pg");
const logging = @import("log");
const bounded_fetch = @import("../bounded_fetch.zig");
const ec = @import("../../../../errors/error_registry.zig");
const vault = @import("../../../../state/vault.zig");
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
/// API root; `sched` is the process deadline scheduler. Never throws — returns a
/// verdict; all failures log `UZ-SLK-030`.
pub fn deliver(
    alloc: std.mem.Allocator,
    io: std.Io,
    sched: *bounded_fetch.Scheduler,
    pool: *pg.Pool,
    api_base: []const u8,
    workspace_id: []const u8,
    fleet_id: []const u8,
    event_id: []const u8,
    answer: []const u8,
) Outcome {
    var loaded = switch (loadInputs(alloc, pool, workspace_id, fleet_id, event_id)) {
        .verdict => |v| return v,
        .ok => |l| l,
    };
    defer loaded.deinit(alloc);
    const robj = switch (loaded.req.value) {
        .object => |o| o,
        else => return .permanent,
    };
    const channel = strField(robj, RQ_CHANNEL) orelse return .permanent;
    const thread_ts = strField(robj, RQ_THREAD) orelse return .permanent;

    return postMessage(alloc, io, sched, api_base, loaded.token, channel, thread_ts, answer);
}

/// Everything the post needs from the database, loaded under ONE short-lived
/// pool acquire that is released before any vendor HTTP begins (a pool slot
/// must never ride a vendor call).
const Loaded = struct {
    req: std.json.Parsed(std.json.Value),
    token: []const u8,

    fn deinit(self: *Loaded, alloc: std.mem.Allocator) void {
        self.req.deinit();
        alloc.free(self.token);
    }
};

const LoadResult = union(enum) { ok: Loaded, verdict: Outcome };

fn loadInputs(
    alloc: std.mem.Allocator,
    pool: *pg.Pool,
    workspace_id: []const u8,
    fleet_id: []const u8,
    event_id: []const u8,
) LoadResult {
    const conn = pool.acquire() catch return .{ .verdict = .retryable }; // DB blip → retry
    defer pool.release(conn);

    // Channel + reply thread live in the event's request_json (what events.zig
    // XADDed). A missing/garbled row is permanent — there is nowhere to post.
    var req = loadRequestJson(alloc, conn, fleet_id, event_id) catch |err| {
        log.warn("slack_post_event_load_failed", .{ .error_code = ec.ERR_SLACK_OUTBOUND_POST_FAILED, .fleet_id = fleet_id, .err = @errorName(err) });
        return .{ .verdict = .permanent };
    } orelse return .{ .verdict = .permanent };

    const token = loadBotToken(alloc, conn, workspace_id) catch |err| {
        // No/garbled handle → uninstalled or misconfigured; a retry won't help.
        req.deinit();
        log.warn("slack_post_token_load_failed", .{ .error_code = ec.ERR_SLACK_OUTBOUND_POST_FAILED, .workspace_id = workspace_id, .err = @errorName(err) });
        return .{ .verdict = .permanent };
    };
    return .{ .ok = .{ .req = req, .token = token } };
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

/// Resolve the per-install bot token from the `(workspace_id,'slack')`
/// vault handle callback.zig wrote (RULE VLT). Caller owns the returned token.
/// `pub` so the mention ingress (`events.zig`) pre-loads the same token for the
/// thread re-read from the one site that reads the `slack` handle
/// (RULE NDC — no second loader).
pub fn loadBotToken(alloc: std.mem.Allocator, conn: *pg.Conn, workspace_id: []const u8) ![]const u8 {
    var parsed = try vault.loadJson(alloc, conn, workspace_id, spec.PROVIDER);
    defer parsed.deinit();
    // Guard the object variant locally rather than relying on loadJson's shape
    // check (mirrors loadSigningSecret / loadAppCreds): loadBotToken runs on the
    // background outbound worker, so a stray non-object handle must return an
    // error the worker classifies `permanent`, never trap the thread.
    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return error.SlackBotTokenHandleMalformed,
    };
    const tok = strField(obj, F_BOT_TOKEN) orelse return error.SlackBotTokenMissing;
    return alloc.dupe(u8, tok);
}

/// POST `{channel, thread_ts, text}` to `chat.postMessage` with the bot token,
/// deadline-armed on the process scheduler. Slack returns 200 with
/// `{ok:true|false}` for app-level outcomes, 429 for rate-limit, 5xx for its
/// own faults — mapped to the retry verdict; a fired deadline or refused call
/// is retryable like any transport failure.
fn postMessage(
    alloc: std.mem.Allocator,
    io: std.Io,
    sched: *bounded_fetch.Scheduler,
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

    const headers = [_]std.http.Header{
        .{ .name = "authorization", .value = auth },
        .{ .name = "content-type", .value = "application/json; charset=utf-8" },
    };
    const res = bounded_fetch.fetch(alloc, io, sched, .{
        .url = url,
        .method = .POST,
        .payload = body,
        .extra_headers = &headers,
        .deadline_ms = bounded_fetch.OUTBOUND_POST_DEADLINE_MS,
        .provider = spec.PROVIDER,
        .class = .outbound_post,
    }) catch |err| {
        log.warn("slack_post_transport_failed", .{ .error_code = ec.ERR_SLACK_OUTBOUND_POST_FAILED, .err = @errorName(err) });
        return .retryable; // transport / deadline / refused → retry with backoff
    };
    defer alloc.free(res.body);
    return classify(alloc, res.status, res.body);
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
