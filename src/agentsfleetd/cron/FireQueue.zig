//! Atomic QStash replay suppression plus Fleet-event append.

const FireQueue = @This();

const std = @import("std");
const error_registry = @import("../errors/error_registry.zig");
const queue_constants = @import("../queue/constants.zig");
const queue_redis = @import("../queue/redis.zig");

const CRON_DEDUP_PREFIX = "cron:dedup:";
const ACTOR_PREFIX = "cron:";
const DECIMAL_FORMAT = "{d}";
const DEDUP_TTL_SECONDS = std.fmt.comptimePrint(DECIMAL_FORMAT, .{error_registry.DEDUP_TTL_SECONDS});
const STREAM_MAX_LENGTH = "10000";
const KEY_BUFFER_BYTES: usize = 384;
const STREAM_KEY_BUFFER_BYTES: usize = 128;
const ACTOR_BUFFER_BYTES: usize = 64;

const DEDUP_AND_APPEND =
    \\if redis.call('EXISTS', KEYS[1]) == 1 or redis.call('EXISTS', KEYS[2]) == 1 then
    \\  return 0
    \\end
    \\local kind = redis.call('TYPE', KEYS[3]).ok
    \\if kind ~= 'none' and kind ~= 'stream' then
    \\  return redis.error_reply('cron event key is not a stream')
    \\end
    \\redis.call('SET', KEYS[1], '1', 'EX', ARGV[1])
    \\redis.call('SET', KEYS[2], '1', 'EX', ARGV[1])
    \\redis.call('XADD', KEYS[3], 'MAXLEN', '~', ARGV[2], '*',
    \\  'type', 'cron', 'actor', ARGV[3], 'workspace_id', ARGV[4],
    \\  'request', ARGV[5], 'created_at', ARGV[6])
    \\return 1
;

alloc: std.mem.Allocator,
client: *queue_redis.Client,

pub const Outcome = enum { enqueued, duplicate };

pub fn init(alloc: std.mem.Allocator, client: *queue_redis.Client) FireQueue {
    return .{ .alloc = alloc, .client = client };
}

pub fn enqueue(
    self: FireQueue,
    fleet_id: []const u8,
    workspace_id: []const u8,
    schedule_id: []const u8,
    signed_message_id: []const u8,
    provider_message_id: []const u8,
    request_json: []const u8,
    created_at_ms: i64,
) !Outcome {
    var signed_key_buffer: [KEY_BUFFER_BYTES]u8 = undefined;
    const signed_key = try dedupKey(&signed_key_buffer, fleet_id, "jwt", signed_message_id);
    var provider_key_buffer: [KEY_BUFFER_BYTES]u8 = undefined;
    const provider_key = try dedupKey(&provider_key_buffer, fleet_id, "msg", provider_message_id);
    var stream_key_buffer: [STREAM_KEY_BUFFER_BYTES]u8 = undefined;
    const stream_key = try std.fmt.bufPrint(&stream_key_buffer, "{s}{s}{s}", .{
        queue_constants.fleet_stream_prefix,
        fleet_id,
        queue_constants.fleet_stream_suffix,
    });
    var actor_buffer: [ACTOR_BUFFER_BYTES]u8 = undefined;
    const actor = try std.fmt.bufPrint(&actor_buffer, "{s}{s}", .{ ACTOR_PREFIX, schedule_id });
    var created_at_buffer: [24]u8 = undefined;
    const created_at = try std.fmt.bufPrint(&created_at_buffer, DECIMAL_FORMAT, .{created_at_ms});

    var response = try self.client.command(&.{
        "EVAL",            DEDUP_AND_APPEND,  "3",   signed_key,   provider_key, stream_key,
        DEDUP_TTL_SECONDS, STREAM_MAX_LENGTH, actor, workspace_id, request_json, created_at,
    });
    defer response.deinit(self.alloc);
    return switch (response) {
        .integer => |value| switch (value) {
            0 => .duplicate,
            1 => .enqueued,
            else => error.RedisUnexpectedResponse,
        },
        else => error.RedisUnexpectedResponse,
    };
}

fn dedupKey(buffer: *[KEY_BUFFER_BYTES]u8, fleet_id: []const u8, kind: []const u8, message_id: []const u8) ![]const u8 {
    return std.fmt.bufPrint(buffer, "{s}{s}:{s}:{s}", .{ CRON_DEDUP_PREFIX, fleet_id, kind, message_id });
}
