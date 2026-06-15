//! Agent Redis stream operations.
//!
//! Operates on per-agent streams (agent:{agent_id}:events) using
//! XREADGROUP / XAUTOCLAIM / XACK with the shared `agent_lease` consumer
//! group. Decoded entries match the EventEnvelope wire shape; field names
//! must stay in lockstep with `contract/event_envelope.zig::encodeForXAdd`.

const std = @import("std");
const logging = @import("log");
const queue_consts = @import("constants.zig");
const redis_protocol = @import("redis_protocol.zig");
const redis_client = @import("redis_client.zig");
const error_codes = @import("../errors/error_registry.zig");
const REDIS_GROUP_ARG = "GROUP";
const REDIS_STREAMS_ARG = "STREAMS";
const REDIS_XREADGROUP_COMMAND = "XREADGROUP";

const log = logging.scoped(.redis_agent);

/// AgentEvent fields decoded from a `agent:{id}:events` stream message.
///
/// `event_id` IS the Redis stream entry id (`<ms>-<seq>`); it is also the
/// argument passed to `xackAgent`. Producer-side does NOT write a
/// separate event_id field — XADD `*` assigns the id and the worker
/// reads it from the stream entry header.
const S_XACK_FAILED = "xack_failed";
const S_COUNT = "COUNT";
/// XREADGROUP id reading the consumer's own PEL from the start ("0") instead
/// of new entries (">").
const PEL_READ_ID = "0";
/// Default for a stream entry that omits the optional workspace_id field.
const EMPTY_WORKSPACE = "";
/// Default request body for an entry that omits the optional request field.
const EMPTY_JSON_BODY = "{}";

pub const AgentEvent = struct {
    const Self = @This();

    event_id: []u8,
    actor: []u8,
    event_type: []u8,
    workspace_id: []u8,
    request_json: []u8,
    created_at_ms: i64,

    pub fn deinit(self: *Self, alloc: std.mem.Allocator) void {
        alloc.free(self.event_id);
        alloc.free(self.actor);
        alloc.free(self.event_type);
        alloc.free(self.workspace_id);
        alloc.free(self.request_json);
    }
};

/// Build the stream key for a agent's event stream.
fn agentStreamKey(buf: []u8, agent_id: []const u8) ![]const u8 {
    return std.fmt.bufPrint(buf, "{s}{s}{s}", .{
        queue_consts.agent_stream_prefix,
        agent_id,
        queue_consts.agent_stream_suffix,
    });
}

/// XGROUP CREATE on a agent's event stream (MKSTREAM, idempotent).
pub fn ensureAgentConsumerGroup(client: *redis_client.Client, agent_id: []const u8) !void {
    var key_buf: [128]u8 = undefined;
    const stream_key = try agentStreamKey(&key_buf, agent_id);
    var resp = try client.commandAllowError(&.{
        "XGROUP",                          "CREATE", stream_key,
        queue_consts.agent_consumer_group, "0",      "MKSTREAM",
    });
    defer resp.deinit(client.alloc);
    switch (resp) {
        .simple => |v| if (!std.mem.eql(u8, v, "OK")) return error.RedisGroupCreateFailed,
        .err => |msg| {
            if (std.mem.indexOf(u8, msg, "BUSYGROUP") != null) return;
            return error.RedisGroupCreateFailed;
        },
        else => return error.RedisGroupCreateFailed,
    }
}

/// XREADGROUP on agent:{id}:events reading the consumer's OWN Pending
/// Entries List (id "0") — re-delivers the oldest entry that was delivered
/// but never XACKed (a pending-gate re-poll, a sweep-recovered strand) before
/// any new event is read. Null when the PEL is empty. Safe against
/// double-leasing: the caller holds the agent's affinity claim, so no other
/// holder can be mid-run on this entry.
pub fn xreadgroupAgentPending(
    client: *redis_client.Client,
    agent_id: []const u8,
    consumer_id: []const u8,
) !?AgentEvent {
    var key_buf: [128]u8 = undefined;
    const stream_key = try agentStreamKey(&key_buf, agent_id);
    var resp = try client.command(&.{
        REDIS_XREADGROUP_COMMAND,          REDIS_GROUP_ARG,
        queue_consts.agent_consumer_group, consumer_id,
        S_COUNT,                           queue_consts.agent_xread_count,
        REDIS_STREAMS_ARG,                 stream_key,
        PEL_READ_ID,
    });
    defer resp.deinit(client.alloc);
    return decodeSingleAgentEvent(client.alloc, resp);
}

/// XREADGROUP on agent:{id}:events WITHOUT BLOCK — returns the next undelivered
/// event immediately, or null. The runner control plane's assignment scan
/// probes many agents per poll and must not park on any single stream; the
/// runner long-polls client-side (via retry_after_ms) instead.
pub fn xreadgroupAgentOnce(
    client: *redis_client.Client,
    agent_id: []const u8,
    consumer_id: []const u8,
) !?AgentEvent {
    var key_buf: [128]u8 = undefined;
    const stream_key = try agentStreamKey(&key_buf, agent_id);
    var resp = try client.command(&.{
        REDIS_XREADGROUP_COMMAND,          REDIS_GROUP_ARG,
        queue_consts.agent_consumer_group, consumer_id,
        S_COUNT,                           queue_consts.agent_xread_count,
        REDIS_STREAMS_ARG,                 stream_key,
        ">",
    });
    defer resp.deinit(client.alloc);
    return decodeSingleAgentEvent(client.alloc, resp);
}

/// XAUTOCLAIM one stale agent event (idle past the comptime-bounded
/// min-idle) into `consumer_id`'s PEL — the recovery half for entries
/// orphaned under a dead consumer name (retired instance, legacy throwaway
/// ids). The claimed entry is then re-delivered by `xreadgroupAgentPending`
/// on the next lease poll.
pub fn xautoclaimAgent(
    client: *redis_client.Client,
    agent_id: []const u8,
    consumer_id: []const u8,
) !?AgentEvent {
    var key_buf: [128]u8 = undefined;
    const stream_key = try agentStreamKey(&key_buf, agent_id);
    var resp = try client.command(&.{
        "XAUTOCLAIM",                              stream_key,
        queue_consts.agent_consumer_group,         consumer_id,
        queue_consts.agent_xautoclaim_min_idle_ms, queue_consts.xautoclaim_start,
        S_COUNT,                                   queue_consts.xautoclaim_count,
    });
    defer resp.deinit(client.alloc);
    return decodeAutoClaimAgentEvent(client.alloc, resp);
}

/// XACK on a agent event stream after successful processing.
pub fn xackAgent(client: *redis_client.Client, agent_id: []const u8, event_id: []const u8) !void {
    var key_buf: [128]u8 = undefined;
    const stream_key = try agentStreamKey(&key_buf, agent_id);
    var resp = try client.command(&.{
        "XACK",                            stream_key,
        queue_consts.agent_consumer_group, event_id,
    });
    defer resp.deinit(client.alloc);
    switch (resp) {
        .integer => |v| if (v < 0) {
            log.err(S_XACK_FAILED, .{ .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED, .agent_id = agent_id, .event_id = event_id });
            return error.RedisXackFailed;
        },
        else => {
            log.err(S_XACK_FAILED, .{ .error_code = error_codes.ERR_INTERNAL_OPERATION_FAILED, .agent_id = agent_id, .event_id = event_id });
            return error.RedisXackFailed;
        },
    }
    log.debug("xack_succeeded", .{ .agent_id = agent_id, .event_id = event_id });
}

// ── Decoders ─────────────────────────────────────────────────────────────

fn decodeSingleAgentEvent(alloc: std.mem.Allocator, value: redis_protocol.RespValue) !?AgentEvent {
    if (value != .array) return null;
    const top = value.array orelse return null;
    if (top.len == 0) return null;
    if (top.len != 1) return error.RedisUnexpectedResponse;
    if (top[0] != .array) return error.RedisUnexpectedResponse;
    const stream_entry = top[0].array orelse return error.RedisUnexpectedResponse;
    if (stream_entry.len != 2) return error.RedisUnexpectedResponse;
    if (stream_entry[1] != .array) return error.RedisUnexpectedResponse;
    const messages = stream_entry[1].array orelse return null;
    if (messages.len == 0) return null;
    return try decodeAgentEventTuple(alloc, messages[0]);
}

fn decodeAutoClaimAgentEvent(alloc: std.mem.Allocator, value: redis_protocol.RespValue) !?AgentEvent {
    if (value != .array) return null;
    const top = value.array orelse return null;
    if (top.len < 2) return error.RedisUnexpectedResponse;
    if (top[1] != .array) return error.RedisUnexpectedResponse;
    const messages = top[1].array orelse return null;
    if (messages.len == 0) return null;
    return try decodeAgentEventTuple(alloc, messages[0]);
}

fn decodeAgentEventTuple(alloc: std.mem.Allocator, item: redis_protocol.RespValue) !AgentEvent {
    if (item != .array) return error.RedisUnexpectedResponse;
    const tuple = item.array orelse return error.RedisUnexpectedResponse;
    if (tuple.len != 2) return error.RedisUnexpectedResponse;
    const event_id_raw = redis_protocol.valueAsString(tuple[0]) orelse return error.RedisUnexpectedResponse;

    if (tuple[1] != .array) return error.RedisUnexpectedResponse;
    const fields = tuple[1].array orelse return error.RedisUnexpectedResponse;
    if (fields.len % 2 != 0) return error.RedisUnexpectedResponse;

    var parsed = try parseEventFields(alloc, fields);
    if (parsed.event_type == null or parsed.actor == null) {
        parsed.freeOwned(alloc);
        return error.RedisUnexpectedResponse;
    }
    return parsed.intoOwned(alloc, event_id_raw);
}

/// Owned (heap-duped) fields decoded from the XADD field array, before the
/// required-field check promotes them into a fully-owned `AgentEvent`. Each
/// non-null slice is caller-owned; `freeOwned` releases whatever is present.
const ParsedFields = struct {
    const Self = @This();

    event_type: ?[]u8 = null,
    actor: ?[]u8 = null,
    workspace_id: ?[]u8 = null,
    request_json: ?[]u8 = null,
    created_at_ms: i64 = 0,

    fn freeOwned(self: Self, alloc: std.mem.Allocator) void {
        if (self.event_type) |e| alloc.free(e);
        if (self.actor) |a| alloc.free(a);
        if (self.workspace_id) |w| alloc.free(w);
        if (self.request_json) |r| alloc.free(r);
    }

    /// Promote to a fully-owned `AgentEvent`. Each dupe lands in a local with
    /// its own `errdefer` so a late OOM frees every already-owned slice — Zig
    /// does not unwind earlier struct-literal fields when a later one errors.
    fn intoOwned(self: Self, alloc: std.mem.Allocator, event_id_raw: []const u8) !AgentEvent {
        const event_type = self.event_type.?;
        errdefer alloc.free(event_type);
        const actor = self.actor.?;
        errdefer alloc.free(actor);
        const workspace_id = self.workspace_id orelse try alloc.dupe(u8, EMPTY_WORKSPACE);
        errdefer alloc.free(workspace_id);
        const request_json = self.request_json orelse try alloc.dupe(u8, EMPTY_JSON_BODY);
        errdefer alloc.free(request_json);
        const event_id = try alloc.dupe(u8, event_id_raw);
        return .{
            .event_id = event_id,
            .actor = actor,
            .event_type = event_type,
            .workspace_id = workspace_id,
            .request_json = request_json,
            .created_at_ms = self.created_at_ms,
        };
    }
};

/// Walk the `[key, val, key, val, …]` field array, duping recognized values.
/// `errdefer freeOwned` unwinds partial dupes if a later `dupe` OOMs — the
/// original inline literal leaked every earlier dupe on a mid-build failure.
fn parseEventFields(alloc: std.mem.Allocator, fields: []const redis_protocol.RespValue) !ParsedFields {
    var out: ParsedFields = .{};
    errdefer out.freeOwned(alloc);
    var i: usize = 0;
    while (i < fields.len) : (i += 2) {
        const key = redis_protocol.valueAsString(fields[i]) orelse continue;
        const val = redis_protocol.valueAsString(fields[i + 1]) orelse continue;

        if (std.mem.eql(u8, key, queue_consts.agent_field_type)) {
            out.event_type = try alloc.dupe(u8, val);
        } else if (std.mem.eql(u8, key, queue_consts.agent_field_actor)) {
            out.actor = try alloc.dupe(u8, val);
        } else if (std.mem.eql(u8, key, queue_consts.agent_field_workspace_id)) {
            out.workspace_id = try alloc.dupe(u8, val);
        } else if (std.mem.eql(u8, key, queue_consts.agent_field_request)) {
            out.request_json = try alloc.dupe(u8, val);
        } else if (std.mem.eql(u8, key, queue_consts.agent_field_created_at)) {
            out.created_at_ms = std.fmt.parseInt(i64, val, 10) catch 0;
        }
    }
    return out;
}

// ── Tests ──────────────────────────────────────────────────────────────────

const testing = std.testing;

fn dupBulk(alloc: std.mem.Allocator, s: []const u8) !redis_protocol.RespValue {
    return .{ .bulk = try alloc.dupe(u8, s) };
}

/// `[id, [type,…,actor,…(,workspace,…,request,…)]]` stream-entry tuple owned by
/// `alloc`. `full` adds the optional workspace_id + request fields; omitting
/// them drives `intoOwned`'s default-dupe path. The testing allocator never
/// fails mid-build, so the whole tuple is always freed via the caller's deinit.
fn buildTuple(alloc: std.mem.Allocator, full: bool) !redis_protocol.RespValue {
    const n: usize = if (full) 8 else 4;
    const fields = try alloc.alloc(redis_protocol.RespValue, n);
    fields[0] = try dupBulk(alloc, queue_consts.agent_field_type);
    fields[1] = try dupBulk(alloc, "run.started");
    fields[2] = try dupBulk(alloc, queue_consts.agent_field_actor);
    fields[3] = try dupBulk(alloc, "steer:x");
    if (full) {
        fields[4] = try dupBulk(alloc, queue_consts.agent_field_workspace_id);
        fields[5] = try dupBulk(alloc, "ws_1");
        fields[6] = try dupBulk(alloc, queue_consts.agent_field_request);
        fields[7] = try dupBulk(alloc, "{\"k\":1}");
    }
    const tuple = try alloc.alloc(redis_protocol.RespValue, 2);
    tuple[0] = try dupBulk(alloc, "1700000000000-0");
    tuple[1] = .{ .array = fields };
    return .{ .array = tuple };
}

fn decodeForLeakCheck(alloc: std.mem.Allocator, item: *const redis_protocol.RespValue) !void {
    var ev = try decodeAgentEventTuple(alloc, item.*);
    ev.deinit(alloc);
}

test "decodeAgentEventTuple unwinds every owned slice on OOM at any step" {
    var item = try buildTuple(testing.allocator, false);
    defer item.deinit(testing.allocator);
    // Fails each internal dupe in turn (event_type, actor, the two defaulted
    // slices, event_id) and asserts the only error is OutOfMemory with zero
    // leaked bytes — proving the parseEventFields + intoOwned errdefer chain.
    try testing.checkAllAllocationFailures(testing.allocator, decodeForLeakCheck, .{&item});
}

test "decodeAgentEventTuple round-trips all present fields" {
    var item = try buildTuple(testing.allocator, true);
    defer item.deinit(testing.allocator);
    var ev = try decodeAgentEventTuple(testing.allocator, item);
    defer ev.deinit(testing.allocator);
    try testing.expectEqualStrings("1700000000000-0", ev.event_id);
    try testing.expectEqualStrings("run.started", ev.event_type);
    try testing.expectEqualStrings("steer:x", ev.actor);
    try testing.expectEqualStrings("ws_1", ev.workspace_id);
    try testing.expectEqualStrings("{\"k\":1}", ev.request_json);
}

test "decodeAgentEventTuple defaults workspace and request when absent" {
    var item = try buildTuple(testing.allocator, false);
    defer item.deinit(testing.allocator);
    var ev = try decodeAgentEventTuple(testing.allocator, item);
    defer ev.deinit(testing.allocator);
    try testing.expectEqualStrings(EMPTY_WORKSPACE, ev.workspace_id);
    try testing.expectEqualStrings(EMPTY_JSON_BODY, ev.request_json);
}
