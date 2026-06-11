//! Normalized event envelope shared by every producer (operator steer,
//! webhook ingest, cron trigger, continuation re-enqueue) and every
//! consumer (control-plane lease/report write-path, history endpoint, SSE handler).
//!
//! The Redis stream entry ID IS the canonical event_id — never carry a
//! separate ID in the payload fields. XADD `*` returns the stream ID;
//! that ID flows into `core.zombie_events.event_id` and out to clients
//! via SSE / GET /events.
//!
//! Encoded on the wire as flat field/value pairs (Redis stream
//! convention). `request_json` is opaque JSON bytes — the worker
//! re-parses it for execution; the read endpoints surface it verbatim.

const EventEnvelope = @This();

const S_CHAT = "chat";
const S_CONTINUATION = "continuation";
const S_CRON = "cron";
const S_WEBHOOK = "webhook";

event_id: []const u8,
zombie_id: []const u8,
workspace_id: []const u8,
actor: []const u8,
event_type: EventType,
request_json: []const u8,
created_at: i64,

pub const EventType = enum {
    chat,
    webhook,
    cron,
    continuation,

    pub fn toSlice(self: EventType) []const u8 {
        return switch (self) {
            .chat => S_CHAT,
            .webhook => S_WEBHOOK,
            .cron => S_CRON,
            .continuation => S_CONTINUATION,
        };
    }

    pub fn fromSlice(s: []const u8) ?EventType {
        if (std.mem.eql(u8, s, S_CHAT)) return .chat;
        if (std.mem.eql(u8, s, S_WEBHOOK)) return .webhook;
        if (std.mem.eql(u8, s, S_CRON)) return .cron;
        if (std.mem.eql(u8, s, S_CONTINUATION)) return .continuation;
        return null;
    }
};


/// Build the XADD field/value argv for a fresh envelope (event_id is
/// assigned by Redis via `*`, so it is omitted here).
///
/// Caller owns the returned slice and each interior string is borrowed
/// from `self` — do not free per-element. Free the outer slice with
/// `alloc.free(result)` after the XADD round-trip completes.
pub fn encodeForXAdd(self: EventEnvelope, alloc: Allocator) ![]const []const u8 {
    var created_at_buf: [24]u8 = undefined;
    const created_at_str = try std.fmt.bufPrint(&created_at_buf, "{d}", .{self.created_at});
    // bufPrint result is stack-borrowed; we need to dupe it because the
    // returned argv outlives this stack frame.
    const created_at_owned = try alloc.dupe(u8, created_at_str);
    errdefer alloc.free(created_at_owned);

    var argv = try alloc.alloc([]const u8, 10);
    argv[0] = "type";
    argv[1] = self.event_type.toSlice();
    argv[2] = "actor";
    argv[3] = self.actor;
    argv[4] = "workspace_id";
    argv[5] = self.workspace_id;
    argv[6] = "request";
    argv[7] = self.request_json;
    argv[8] = "created_at";
    argv[9] = created_at_owned;
    return argv;
}

/// Free the argv built by `encodeForXAdd`. Assumes index 9 is the only
/// owned element (the formatted `created_at`).
pub fn freeXAddArgv(alloc: Allocator, argv: []const []const u8) void {
    if (argv.len >= 10) alloc.free(argv[9]);
    alloc.free(argv);
}

/// Compute the continuation actor for a re-enqueued event. Flat — never
/// re-nests `continuation:`. A steer that chunks 3 times produces
/// `actor=continuation:steer:kishore` on every continuation, not
/// `continuation:continuation:continuation:steer:kishore`.
///

const std = @import("std");
const Allocator = std.mem.Allocator;
