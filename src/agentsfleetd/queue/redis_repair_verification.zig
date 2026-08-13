//! Redis enqueue-once primitive for durable repair-verification intents.

const std = @import("std");

const queue_consts = @import("constants.zig");
const redis_client = @import("redis_client.zig");
const redis_fleet_probe = @import("redis_fleet_probe.zig");
const redis_protocol = @import("redis_protocol.zig");
const fleet_ready = @import("fleet_ready.zig");
const EventEnvelope = @import("contract").event_envelope;

const ONCE_PREFIX = "fleet:repair-verification:";
const ONCE_KEY_BUF_LEN: usize = 128;
const ONCE_KEY_FORMAT = "{s}{s}";
const EVAL_COMMAND = "EVAL";
const TWO_KEYS = "2";
const XADD_MAXLEN = "10000";
const PREFIX_LEN: usize = 6;
const EMITTED = "emitted";
const REPLAYED = "replayed";

const XADD_ONCE =
    \\local existing = redis.call('GET', KEYS[1])
    \\if existing then return {existing, 'replayed'} end
    \\local kind = redis.call('TYPE', KEYS[2]).ok
    \\if kind ~= 'none' and kind ~= 'stream' then
    \\  return redis.error_reply('repair verification event key is not a stream')
    \\end
    \\local event_id = redis.call('XADD', KEYS[2], 'MAXLEN', '~', ARGV[1], '*', unpack(ARGV, 2))
    \\redis.call('SET', KEYS[1], event_id)
    \\return {event_id, 'emitted'}
;

pub const Enqueue = struct {
    event_id: []u8,
    queued_at_ms: i64,
    replayed: bool,
};

/// Atomically append one Fleet event for a durable intent. A retry returns the
/// original Redis stream identifier; caller must free it with `client.alloc`.
pub fn xaddOnce(client: *redis_client.Client, verification_id: []const u8, envelope: EventEnvelope) !Enqueue {
    var once_key_buf: [ONCE_KEY_BUF_LEN]u8 = undefined;
    const once_key = try std.fmt.bufPrint(&once_key_buf, ONCE_KEY_FORMAT, .{ ONCE_PREFIX, verification_id });
    var stream_key_buf: [queue_consts.fleet_stream_key_buf_len]u8 = undefined;
    const stream_key = try queue_consts.fleetStreamKey(&stream_key_buf, envelope.fleet_id);
    const payload_argv = try envelope.encodeForXAdd(client.alloc);
    defer EventEnvelope.freeXAddArgv(client.alloc, payload_argv);
    const argv = try client.alloc.alloc([]const u8, PREFIX_LEN + payload_argv.len);
    defer client.alloc.free(argv);
    argv[0] = EVAL_COMMAND;
    argv[1] = XADD_ONCE;
    argv[2] = TWO_KEYS;
    argv[3] = once_key;
    argv[4] = stream_key;
    argv[5] = XADD_MAXLEN;
    @memcpy(argv[PREFIX_LEN..], payload_argv);
    var response = try client.command(argv);
    defer response.deinit(client.alloc);
    const values = switch (response) {
        .array => |value| value orelse return error.RedisXaddFailed,
        else => return error.RedisXaddFailed,
    };
    if (values.len != 2) return error.RedisXaddFailed;
    const event_id = redis_protocol.valueAsString(values[0]) orelse return error.RedisXaddFailed;
    const outcome = redis_protocol.valueAsString(values[1]) orelse return error.RedisXaddFailed;
    const replayed = if (std.mem.eql(u8, outcome, REPLAYED)) true else if (std.mem.eql(u8, outcome, EMITTED)) false else return error.RedisXaddFailed;
    const stream_id = redis_fleet_probe.parseStreamId(event_id) catch return error.RedisXaddFailed;
    const queued_at_ms = std.math.cast(i64, stream_id.ms) orelse return error.RedisXaddFailed;
    fleet_ready.mark(client, envelope.fleet_id);
    return .{
        .event_id = try client.alloc.dupe(u8, event_id),
        .queued_at_ms = queued_at_ms,
        .replayed = replayed,
    };
}

/// Forget an enqueue-once key only after its durable verifier event link exists.
/// A replay before that completion must keep returning the original stream ID.
pub fn clearOnce(client: *redis_client.Client, verification_id: []const u8) !void {
    var once_key_buf: [ONCE_KEY_BUF_LEN]u8 = undefined;
    const once_key = try std.fmt.bufPrint(&once_key_buf, ONCE_KEY_FORMAT, .{ ONCE_PREFIX, verification_id });
    try client.del(once_key);
}
