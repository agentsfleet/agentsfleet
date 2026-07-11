//! `connector:outbound` durable job stream — the generic, provider-routed
//! answer-delivery queue (§4).
//!
//! When a completed run's fleet has a connector binding, `service_report`
//! enqueues a job here; a boot-started consumer thread
//! (`http/handlers/connectors/outbound/worker.zig`) reads jobs and dispatches by
//! `provider` to the matching connector poster (Slack now; Grafana/Jira/Linear
//! later add a `switch` arm). Pure Redis stream ops (mirrors `redis_fleet.zig`)
//! and provider-agnostic — the job carries `provider` as an opaque string, so
//! the core report path never imports a connector (Invariant 9). Durable: a
//! consumer-group stream survives an agentsfleetd restart and redelivers unacked
//! jobs.

const std = @import("std");
const redis_protocol = @import("redis_protocol.zig");
const redis_client = @import("redis_client.zig");

pub const STREAM_KEY = "connector:outbound";
pub const CONSUMER_GROUP = "connector_workers";
/// Approximate stream cap: a wedged consumer can never grow the stream unbounded.
const MAXLEN_CAP = "100000";
const F_PROVIDER = "provider";
const F_WORKSPACE_ID = "workspace_id";
const F_FLEET_ID = "fleet_id";
const F_EVENT_ID = "event_id";
const F_ANSWER = "answer";
// Redis command tokens reused across readNext/readPending (RULE UFS — a repeated
// literal is a named const; mirrors redis_fleet.zig's REDIS_*_ARG consts).
const CMD_XREADGROUP = "XREADGROUP";
const ARG_GROUP = "GROUP";
const ARG_COUNT = "COUNT";
const ARG_STREAMS = "STREAMS";
const READ_COUNT = "1";

/// One outbound delivery job. `provider` routes it to a connector poster; the
/// rest let the poster resolve the target (channel/thread from the event's
/// `request_json`) + the bot token (from the workspace vault handle).
pub const Job = struct {
    provider: []const u8,
    workspace_id: []const u8,
    fleet_id: []const u8,
    event_id: []const u8,
    answer: []const u8,
};

/// A decoded delivery: the Redis entry id (for XACK) + owned job fields.
pub const Delivery = struct {
    const Self = @This();
    entry_id: []u8,
    provider: []u8,
    workspace_id: []u8,
    fleet_id: []u8,
    event_id: []u8,
    answer: []u8,

    pub fn deinit(self: *Self, alloc: std.mem.Allocator) void {
        alloc.free(self.entry_id);
        alloc.free(self.provider);
        alloc.free(self.workspace_id);
        alloc.free(self.fleet_id);
        alloc.free(self.event_id);
        alloc.free(self.answer);
    }
};

/// XGROUP CREATE MKSTREAM on the shared outbound stream (idempotent). Call once
/// at boot before the consumer reads.
pub fn ensureGroup(client: *redis_client.Client) !void {
    var resp = try client.commandAllowError(&.{
        "XGROUP", "CREATE", STREAM_KEY, CONSUMER_GROUP, "0", "MKSTREAM",
    });
    defer resp.deinit(client.alloc);
    switch (resp) {
        .simple => |v| if (!std.mem.eql(u8, v, "OK")) return error.RedisGroupCreateFailed,
        .err => |msg| {
            if (std.mem.indexOf(u8, msg, "BUSYGROUP") != null) return; // already exists
            return error.RedisGroupCreateFailed;
        },
        else => return error.RedisGroupCreateFailed,
    }
}

/// XADD a job (id auto-assigned by `*`). Returns the entry id (caller owns).
pub fn enqueue(client: *redis_client.Client, job: Job) ![]u8 {
    var resp = try client.command(&.{
        "XADD",     STREAM_KEY,   "MAXLEN",       "~",              MAXLEN_CAP, "*",
        F_PROVIDER, job.provider, F_WORKSPACE_ID, job.workspace_id, F_FLEET_ID, job.fleet_id,
        F_EVENT_ID, job.event_id, F_ANSWER,       job.answer,
    });
    defer resp.deinit(client.alloc);
    const id = switch (resp) {
        .bulk => |v| v orelse return error.RedisXaddFailed,
        else => return error.RedisXaddFailed,
    };
    return client.alloc.dupe(u8, id);
}

/// Non-blocking XREADGROUP for the next undelivered job — null immediately on
/// an idle stream. The worker paces itself with an idle backoff instead of a
/// server-side BLOCK, so the shared queue connection is borrowed per-command
/// like every other pooled read (the lease-path shape) rather than parked on
/// the stream.
pub fn readNext(client: *redis_client.Client, consumer_id: []const u8) !?Delivery {
    var resp = try client.command(&.{
        CMD_XREADGROUP, ARG_GROUP,  CONSUMER_GROUP, consumer_id,
        ARG_COUNT,      READ_COUNT, ARG_STREAMS,    STREAM_KEY,
        ">",
    });
    defer resp.deinit(client.alloc);
    return decodeDelivery(client.alloc, resp);
}

/// XREADGROUP reading this consumer's OWN pending entries (id "0", non-blocking)
/// — jobs delivered but not yet XACKed (e.g. left pending by an agentsfleetd
/// restart mid-post). Null when the PEL is empty. The consumer id must be stable
/// across restarts (`stableConsumerId`) for this to reclaim prior work.
pub fn readPending(client: *redis_client.Client, consumer_id: []const u8) !?Delivery {
    var resp = try client.command(&.{
        CMD_XREADGROUP, ARG_GROUP,  CONSUMER_GROUP, consumer_id,
        ARG_COUNT,      READ_COUNT, ARG_STREAMS,    STREAM_KEY,
        "0",
    });
    defer resp.deinit(client.alloc);
    return decodeDelivery(client.alloc, resp);
}

/// XACK a delivered job after successful processing (or a permanent drop).
pub fn ack(client: *redis_client.Client, entry_id: []const u8) !void {
    var resp = try client.command(&.{ "XACK", STREAM_KEY, CONSUMER_GROUP, entry_id });
    defer resp.deinit(client.alloc);
    switch (resp) {
        .integer => |v| if (v < 0) return error.RedisXackFailed,
        else => return error.RedisXackFailed,
    }
}

// ── Decoders (mirror redis_fleet.zig's XREADGROUP nesting) ──────────────────

fn decodeDelivery(alloc: std.mem.Allocator, value: redis_protocol.RespValue) !?Delivery {
    if (value != .array) return null;
    const top = value.array orelse return null; // nil = BLOCK timeout, no work
    if (top.len == 0) return null;
    if (top.len != 1) return error.RedisUnexpectedResponse;
    if (top[0] != .array) return error.RedisUnexpectedResponse;
    const stream_entry = top[0].array orelse return error.RedisUnexpectedResponse;
    if (stream_entry.len != 2) return error.RedisUnexpectedResponse;
    if (stream_entry[1] != .array) return error.RedisUnexpectedResponse;
    const messages = stream_entry[1].array orelse return null;
    if (messages.len == 0) return null;
    return try decodeJobTuple(alloc, messages[0]);
}

fn decodeJobTuple(alloc: std.mem.Allocator, item: redis_protocol.RespValue) !Delivery {
    if (item != .array) return error.RedisUnexpectedResponse;
    const tuple = item.array orelse return error.RedisUnexpectedResponse;
    if (tuple.len != 2) return error.RedisUnexpectedResponse;
    const entry_id_raw = redis_protocol.valueAsString(tuple[0]) orelse return error.RedisUnexpectedResponse;

    if (tuple[1] != .array) return error.RedisUnexpectedResponse;
    const fields = tuple[1].array orelse return error.RedisUnexpectedResponse;
    if (fields.len % 2 != 0) return error.RedisUnexpectedResponse;

    var parsed = try parseJobFields(alloc, fields);
    // Every field is written by `enqueue`, so all are required — a missing one
    // is a malformed job.
    if (parsed.provider == null or parsed.workspace_id == null or parsed.fleet_id == null or
        parsed.event_id == null or parsed.answer == null)
    {
        parsed.freeOwned(alloc);
        return error.RedisUnexpectedResponse;
    }
    return parsed.intoOwned(alloc, entry_id_raw);
}

/// Owned (heap-duped) fields before the required-field check promotes them to a
/// fully-owned `Delivery`. Each non-null slice is caller-owned; `freeOwned`
/// releases whatever is present.
const ParsedJobFields = struct {
    const Self = @This();
    provider: ?[]u8 = null,
    workspace_id: ?[]u8 = null,
    fleet_id: ?[]u8 = null,
    event_id: ?[]u8 = null,
    answer: ?[]u8 = null,

    fn freeOwned(self: Self, alloc: std.mem.Allocator) void {
        if (self.provider) |v| alloc.free(v);
        if (self.workspace_id) |v| alloc.free(v);
        if (self.fleet_id) |v| alloc.free(v);
        if (self.event_id) |v| alloc.free(v);
        if (self.answer) |v| alloc.free(v);
    }

    /// Promote to a fully-owned `Delivery`. Each dupe lands in a local with its
    /// own `errdefer` so a late OOM frees every already-owned slice (Zig does not
    /// unwind earlier struct-literal fields when a later one errors).
    fn intoOwned(self: Self, alloc: std.mem.Allocator, entry_id_raw: []const u8) !Delivery {
        const provider = self.provider.?;
        errdefer alloc.free(provider);
        const workspace_id = self.workspace_id.?;
        errdefer alloc.free(workspace_id);
        const fleet_id = self.fleet_id.?;
        errdefer alloc.free(fleet_id);
        const event_id = self.event_id.?;
        errdefer alloc.free(event_id);
        const answer = self.answer.?;
        errdefer alloc.free(answer);
        const entry_id = try alloc.dupe(u8, entry_id_raw);
        return .{
            .entry_id = entry_id,
            .provider = provider,
            .workspace_id = workspace_id,
            .fleet_id = fleet_id,
            .event_id = event_id,
            .answer = answer,
        };
    }
};

/// Dupe `val` into `slot`, freeing any prior value: a repeated stream key
/// (foreign writer / operator tooling) must not orphan the earlier dupe.
/// Dupe-before-free so an OOM leaves the previous value owned by the struct
/// for `errdefer freeOwned` to release.
fn setDuped(alloc: std.mem.Allocator, slot: *?[]u8, val: []const u8) !void {
    const duped = try alloc.dupe(u8, val);
    if (slot.*) |prev| alloc.free(prev);
    slot.* = duped;
}

/// Walk the `[key, val, …]` field array, duping recognized values. `errdefer
/// freeOwned` unwinds partial dupes if a later `dupe` OOMs; a repeated key
/// replaces (and frees) the earlier value — last write wins.
fn parseJobFields(alloc: std.mem.Allocator, fields: []const redis_protocol.RespValue) !ParsedJobFields {
    var out: ParsedJobFields = .{};
    errdefer out.freeOwned(alloc);
    var i: usize = 0;
    while (i < fields.len) : (i += 2) {
        const key = redis_protocol.valueAsString(fields[i]) orelse continue;
        const val = redis_protocol.valueAsString(fields[i + 1]) orelse continue;
        if (std.mem.eql(u8, key, F_PROVIDER)) {
            try setDuped(alloc, &out.provider, val);
        } else if (std.mem.eql(u8, key, F_WORKSPACE_ID)) {
            try setDuped(alloc, &out.workspace_id, val);
        } else if (std.mem.eql(u8, key, F_FLEET_ID)) {
            try setDuped(alloc, &out.fleet_id, val);
        } else if (std.mem.eql(u8, key, F_EVENT_ID)) {
            try setDuped(alloc, &out.event_id, val);
        } else if (std.mem.eql(u8, key, F_ANSWER)) {
            try setDuped(alloc, &out.answer, val);
        }
    }
    return out;
}

// ── Tests ────────────────────────────────────────────────────────────────────

const testing = std.testing;

fn dupBulk(alloc: std.mem.Allocator, s: []const u8) !redis_protocol.RespValue {
    return .{ .bulk = try alloc.dupe(u8, s) };
}

/// `[entry_id, [provider,…,workspace_id,…,fleet_id,…,event_id,…,answer,…]]`.
fn buildTuple(alloc: std.mem.Allocator) !redis_protocol.RespValue {
    const fields = try alloc.alloc(redis_protocol.RespValue, 10);
    fields[0] = try dupBulk(alloc, F_PROVIDER);
    fields[1] = try dupBulk(alloc, "slack");
    fields[2] = try dupBulk(alloc, F_WORKSPACE_ID);
    fields[3] = try dupBulk(alloc, "ws_1");
    fields[4] = try dupBulk(alloc, F_FLEET_ID);
    fields[5] = try dupBulk(alloc, "fleet_1");
    fields[6] = try dupBulk(alloc, F_EVENT_ID);
    fields[7] = try dupBulk(alloc, "1700000000000-0");
    fields[8] = try dupBulk(alloc, F_ANSWER);
    fields[9] = try dupBulk(alloc, "Aurora is healthy.");
    const tuple = try alloc.alloc(redis_protocol.RespValue, 2);
    tuple[0] = try dupBulk(alloc, "1700000000001-0");
    tuple[1] = .{ .array = fields };
    return .{ .array = tuple };
}

fn decodeForLeakCheck(alloc: std.mem.Allocator, item: *const redis_protocol.RespValue) !void {
    var d = try decodeJobTuple(alloc, item.*);
    d.deinit(alloc);
}

test "decodeJobTuple round-trips all job fields + entry id" {
    var item = try buildTuple(testing.allocator);
    defer item.deinit(testing.allocator);
    var d = try decodeJobTuple(testing.allocator, item);
    defer d.deinit(testing.allocator);
    try testing.expectEqualStrings("1700000000001-0", d.entry_id);
    try testing.expectEqualStrings("slack", d.provider);
    try testing.expectEqualStrings("ws_1", d.workspace_id);
    try testing.expectEqualStrings("fleet_1", d.fleet_id);
    try testing.expectEqualStrings("1700000000000-0", d.event_id);
    try testing.expectEqualStrings("Aurora is healthy.", d.answer);
}

test "decodeJobTuple unwinds every owned slice on OOM at any step" {
    var item = try buildTuple(testing.allocator);
    defer item.deinit(testing.allocator);
    try testing.checkAllAllocationFailures(testing.allocator, decodeForLeakCheck, .{&item});
}

fn parseDupKeyForLeakCheck(alloc: std.mem.Allocator, fields: []const redis_protocol.RespValue) !void {
    var parsed = try parseJobFields(alloc, fields);
    parsed.freeOwned(alloc);
}

test "parseJobFields frees the earlier dupe on a repeated key (last wins)" {
    const alloc = testing.allocator;
    const fields = try alloc.alloc(redis_protocol.RespValue, 4);
    fields[0] = try dupBulk(alloc, F_PROVIDER);
    fields[1] = try dupBulk(alloc, "slack");
    fields[2] = try dupBulk(alloc, F_PROVIDER);
    fields[3] = try dupBulk(alloc, "github");
    var item = redis_protocol.RespValue{ .array = fields };
    defer item.deinit(alloc);

    var parsed = try parseJobFields(alloc, fields);
    defer parsed.freeOwned(alloc);
    try testing.expectEqualStrings("github", parsed.provider.?);

    // OOM at every allocation point with a prior dupe owned: nothing leaks.
    try testing.checkAllAllocationFailures(alloc, parseDupKeyForLeakCheck, .{fields});
}

test "decodeJobTuple rejects a job missing a required field" {
    const fields = try testing.allocator.alloc(redis_protocol.RespValue, 2); // provider only
    fields[0] = try dupBulk(testing.allocator, F_PROVIDER);
    fields[1] = try dupBulk(testing.allocator, "slack");
    const tuple = try testing.allocator.alloc(redis_protocol.RespValue, 2);
    tuple[0] = try dupBulk(testing.allocator, "1-0");
    tuple[1] = .{ .array = fields };
    var item = redis_protocol.RespValue{ .array = tuple };
    defer item.deinit(testing.allocator);
    try testing.expectError(error.RedisUnexpectedResponse, decodeJobTuple(testing.allocator, item));
}
