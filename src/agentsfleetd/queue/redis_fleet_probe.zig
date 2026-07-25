//! "Does this fleet still hold anything a runner could pick up?"
//!
//! The reclaim sweeper asks this per fleet so it can re-mark readiness for work
//! the index lost. Two things count as deliverable, and the sweeper needs both:
//!
//!   1. **A non-empty Pending Entries List (PEL)** — an entry delivered to some
//!      consumer and never acknowledged. `XAUTOCLAIM` alone only reaches these
//!      once they are idle past the min-idle bound, so checking the count
//!      directly recovers another replica's strand a full sweep sooner.
//!   2. **An undelivered entry** — appended to the stream but never handed to any
//!      consumer. This is the case `XAUTOCLAIM` can NEVER see, because the entry
//!      is in nobody's pending list. It is also the exact shape of the worst
//!      failure the readiness index has: a successful append whose readiness mark
//!      then failed. Without this half, that event is stranded permanently.
//!
//! **Why not the consumer group's `lag` field.** `lag` is deliberately null
//! whenever entries have been deleted or trimmed, because the server can no
//! longer count them exactly. Ingress trims at `MAXLEN ~ 10000`, so a production
//! stream reaches that state as a matter of course and a lag-based probe would
//! silently stop answering. Comparing the group's `last-delivered-id` against the
//! stream's `last-generated-id` is always defined, and needs no Redis 7 feature.
//!
//! Trimming cannot make that comparison lie in the unsafe direction:
//! `last-delivered-id` only advances when an entry is handed out, so if every
//! entry has been delivered the two ids are equal regardless of what was trimmed
//! away afterwards.

const std = @import("std");
const queue_consts = @import("constants.zig");
const redis_protocol = @import("redis_protocol.zig");
const redis_client = @import("redis_client.zig");

const CMD_XINFO = "XINFO";
const ARG_GROUPS = "GROUPS";
const ARG_STREAM = "STREAM";
const FIELD_NAME = "name";
const FIELD_PENDING = "pending";
const FIELD_LAST_DELIVERED = "last-delivered-id";
const FIELD_LAST_GENERATED = "last-generated-id";
const FIELD_LENGTH = "length";
/// The id Redis reports before anything has been generated or delivered.
const STREAM_ID_ZERO = "0-0";
const STREAM_ID_SEPARATOR = '-';

/// A Redis stream entry id: `<milliseconds>-<sequence>`.
///
/// Held as two integers rather than compared as text, because the text order is
/// NOT the real order — `"100-0"` sorts before `"99-0"` lexicographically while
/// being the strictly later entry. A natural string comparison here gets that
/// silently wrong, which is why this type and its test exist.
pub const StreamId = struct {
    ms: u64,
    seq: u64,

    pub fn lessThan(self: StreamId, other: StreamId) bool {
        if (self.ms != other.ms) return self.ms < other.ms;
        return self.seq < other.seq;
    }
};

/// Parse `<ms>-<seq>`. A bare `<ms>` with no separator is accepted with sequence
/// zero, matching how Redis expands a partial id.
pub fn parseStreamId(text: []const u8) !StreamId {
    const sep = std.mem.indexOfScalar(u8, text, STREAM_ID_SEPARATOR) orelse {
        return .{ .ms = try std.fmt.parseInt(u64, text, 10), .seq = 0 };
    };
    return .{
        .ms = try std.fmt.parseInt(u64, text[0..sep], 10),
        .seq = try std.fmt.parseInt(u64, text[sep + 1 ..], 10),
    };
}

/// True when the fleet holds a pending or an undelivered entry.
///
/// A missing stream or a Redis failure answers `false` rather than erroring: this
/// feeds a best-effort re-mark, and the caller must not abandon its pass over the
/// remaining fleets because one stream was absent. False is also the safe answer
/// for a genuinely absent stream — there is nothing there to strand.
pub fn hasDeliverable(client: *redis_client.Client, fleet_id: []const u8) bool {
    var key_buf: [queue_consts.fleet_stream_key_buf_len]u8 = undefined;
    const stream_key = queue_consts.fleetStreamKey(&key_buf, fleet_id) catch return false;

    const stream = readStreamState(client, stream_key) catch return false;
    // No entries ever generated ⇒ nothing to deliver, whatever the group says.
    if (stream.length == 0) return false;

    const group = readGroupState(client, stream_key) catch return false;
    const delivered = group orelse {
        // No consumer group yet ⇒ no runner has ever read this fleet, so every
        // entry present is undelivered.
        return true;
    };
    if (delivered.pending > 0) return true;
    return delivered.last_delivered.lessThan(stream.last_generated);
}

const StreamState = struct {
    length: u64,
    last_generated: StreamId,
};

const GroupState = struct {
    pending: u64,
    last_delivered: StreamId,
};

fn readStreamState(client: *redis_client.Client, stream_key: []const u8) !StreamState {
    var resp = try client.command(&.{ CMD_XINFO, ARG_STREAM, stream_key });
    defer resp.deinit(client.alloc);
    if (resp != .array) return error.RedisUnexpectedResponse;
    const flat = resp.array orelse return error.RedisUnexpectedResponse;
    return .{
        .length = integerField(flat, FIELD_LENGTH) orelse 0,
        .last_generated = try parseStreamId(stringField(flat, FIELD_LAST_GENERATED) orelse STREAM_ID_ZERO),
    };
}

/// Null when this stream carries no `fleet_lease` group yet.
fn readGroupState(client: *redis_client.Client, stream_key: []const u8) !?GroupState {
    var resp = try client.command(&.{ CMD_XINFO, ARG_GROUPS, stream_key });
    defer resp.deinit(client.alloc);
    if (resp != .array) return null;
    const groups = resp.array orelse return null;
    for (groups) |entry| {
        if (entry != .array) continue;
        const flat = entry.array orelse continue;
        const name = stringField(flat, FIELD_NAME) orelse continue;
        if (!std.mem.eql(u8, name, queue_consts.fleet_consumer_group)) continue;
        return .{
            .pending = integerField(flat, FIELD_PENDING) orelse 0,
            .last_delivered = try parseStreamId(stringField(flat, FIELD_LAST_DELIVERED) orelse STREAM_ID_ZERO),
        };
    }
    return null;
}

/// `XINFO` replies are flat key/value arrays under RESP2, which is the only
/// protocol this client speaks (it sends no `HELLO`), so scanning pairs is the
/// correct shape rather than a fallback.
fn valueField(flat: []const redis_protocol.RespValue, key: []const u8) ?redis_protocol.RespValue {
    var i: usize = 0;
    while (i + 1 < flat.len) : (i += 2) {
        const found = redis_protocol.valueAsString(flat[i]) orelse continue;
        if (std.mem.eql(u8, found, key)) return flat[i + 1];
    }
    return null;
}

fn stringField(flat: []const redis_protocol.RespValue, key: []const u8) ?[]const u8 {
    return redis_protocol.valueAsString(valueField(flat, key) orelse return null);
}

fn integerField(flat: []const redis_protocol.RespValue, key: []const u8) ?u64 {
    return switch (valueField(flat, key) orelse return null) {
        .integer => |n| if (n > 0) @intCast(n) else 0,
        else => null,
    };
}

test {
    _ = @import("redis_fleet_probe_test.zig");
}
