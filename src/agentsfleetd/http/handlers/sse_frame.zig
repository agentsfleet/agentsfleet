//! Server-Sent Events frame writing, shared by the per-fleet tail and the
//! workspace multiplex.
//!
//! One frame is `id:` (per-connection sequence) + `event:` (the payload's
//! `kind`) + `data:` (the publisher's JSON). The multiplexed variant splices
//! the originating `fleet_id` into `data` so a client watching many fleets over
//! one connection can route each frame to the right tile.
//!
//! Stateless namespace — a frame is bytes on a writer, not a type with state.

const std = @import("std");

/// Idle wake-up cadence for the subscription pop. Each tick with no frames
/// sends a heartbeat comment so a vanished client is detected by the failing
/// write — without it a stream over a dead client would hold its thread and
/// slot until a publish that may never come.
pub const HEARTBEAT_INTERVAL_MS: u32 = 15_000;

/// SSE comment frame — ignored by EventSource clients, but the write probes
/// client liveness and keeps intermediaries from idling the connection out.
pub const HEARTBEAT_FRAME = ": heartbeat\n\n";

/// Fallback `event:` name when the publisher's payload carries no leading
/// `kind` field.
pub const DEFAULT_KIND = "message";

/// The key the multiplexed stream splices into every frame's payload.
const FLEET_ID_KEY = "\"fleet_id\":\"";

/// Blank line terminating an SSE frame's `data:` field.
const FRAME_END = "\n\n";

const SEQ_BUF_LEN: usize = 24;

/// Extract the `kind` field from the JSON payload so the SSE `event:` line can
/// carry it. Anchors on the leading `{"kind":"` prefix so an embedded
/// `"kind":"` inside a string field cannot poison the dispatch. Best-effort —
/// returns null if the publisher's shape changes.
///
/// Callers that also splice a field into the payload must call this FIRST: the
/// anchor is the leading field, and a splice-first payload no longer has `kind`
/// in front.
pub fn extractKind(payload: []const u8) ?[]const u8 {
    const prefix = "{\"kind\":\"";
    if (payload.len < prefix.len) return null;
    if (!std.mem.startsWith(u8, payload, prefix)) return null;
    const close = std.mem.indexOfScalarPos(u8, payload, prefix.len, '"') orelse return null;
    return payload[prefix.len..close];
}

pub fn writeFrame(w: anytype, seq: u64, kind: []const u8, data_json: []const u8) !void {
    try writeHead(w, seq, kind);
    try w.interface.writeAll(data_json);
    try w.interface.writeAll(FRAME_END);
}

/// The multiplexed frame: the publisher's payload with `"fleet_id":"<id>"`
/// spliced in as the leading field, so `data` is a single JSON object the
/// client demultiplexes by fleet.
///
/// `payload` must be a JSON object (`{…}`); a payload that is not is
/// `error.NotAnObject` — the caller drops it rather than emitting a frame that
/// could be routed to the wrong tile.
pub fn writeTaggedFrame(
    w: anytype,
    seq: u64,
    kind: []const u8,
    fleet_id: []const u8,
    payload: []const u8,
) !void {
    if (payload.len < 2 or payload[0] != '{' or payload[payload.len - 1] != '}') return error.NotAnObject;
    try writeHead(w, seq, kind);
    try w.interface.writeAll("{");
    try w.interface.writeAll(FLEET_ID_KEY);
    try w.interface.writeAll(fleet_id);
    try w.interface.writeAll("\"");
    // `{}` has no fields to keep; anything else keeps every byte after the
    // opening brace, so the publisher's payload crosses the wire unrewritten.
    if (payload[1] != '}') {
        try w.interface.writeAll(",");
        try w.interface.writeAll(payload[1..]);
    } else {
        try w.interface.writeAll("}");
    }
    try w.interface.writeAll(FRAME_END);
}

fn writeHead(w: anytype, seq: u64, kind: []const u8) !void {
    var seq_buf: [SEQ_BUF_LEN]u8 = undefined;
    const seq_str = try std.fmt.bufPrint(&seq_buf, "{d}", .{seq});
    try w.interface.writeAll("id: ");
    try w.interface.writeAll(seq_str);
    try w.interface.writeAll("\nevent: ");
    try w.interface.writeAll(kind);
    try w.interface.writeAll("\ndata: ");
}

// ── Tests ───────────────────────────────────────────────────────────────────

const testing = std.testing;

test "extractKind: parses leading kind field" {
    try testing.expectEqualStrings("event_received", extractKind("{\"kind\":\"event_received\",\"event_id\":\"x\"}").?);
    try testing.expectEqualStrings("chunk", extractKind("{\"kind\":\"chunk\",\"text\":\"hi\"}").?);
}

test "extractKind: returns null when field missing" {
    try testing.expect(extractKind("{\"foo\":\"bar\"}") == null);
}

test "extractKind: ignores embedded kind inside a string value" {
    // If a chunk's text happens to contain the kind-needle literal, the
    // anchored prefix scan must not pick it up — the real kind comes
    // first per the publisher's frame shape.
    const poisoned = "{\"kind\":\"chunk\",\"text\":\"\\\"kind\\\":\\\"fake\\\"\"}";
    try testing.expectEqualStrings("chunk", extractKind(poisoned).?);
}

test "extractKind: returns null when kind is not the leading field" {
    try testing.expect(extractKind("{\"event_id\":\"x\",\"kind\":\"chunk\"}") == null);
}

test "extractKind: handles short payloads without panicking" {
    try testing.expect(extractKind("") == null);
    try testing.expect(extractKind("{\"k\"") == null);
}

/// Collects frame bytes through the same `w.interface.writeAll` shape the
/// stream writer exposes, so the writers are tested against their real call
/// shape rather than a bespoke sink.
const FrameSink = struct {
    interface: Interface,

    const Interface = struct {
        out: *std.ArrayList(u8),
        alloc: std.mem.Allocator,

        fn writeAll(self: Interface, bytes: []const u8) !void {
            try self.out.appendSlice(self.alloc, bytes);
        }
    };
};

fn sink(out: *std.ArrayList(u8)) FrameSink {
    return .{ .interface = .{ .out = out, .alloc = testing.allocator } };
}

test "writeFrame: id, event, and the untouched payload" {
    var out: std.ArrayList(u8) = .empty;
    defer out.deinit(testing.allocator);
    var w = sink(&out);

    try writeFrame(&w, 7, "chunk", "{\"kind\":\"chunk\",\"text\":\"hi\"}");
    try testing.expectEqualStrings(
        "id: 7\nevent: chunk\ndata: {\"kind\":\"chunk\",\"text\":\"hi\"}\n\n",
        out.items,
    );
}

test "writeTaggedFrame: splices fleet_id ahead of the publisher's fields" {
    var out: std.ArrayList(u8) = .empty;
    defer out.deinit(testing.allocator);
    var w = sink(&out);

    try writeTaggedFrame(&w, 0, "chunk", "z1", "{\"kind\":\"chunk\",\"text\":\"hi\"}");
    try testing.expectEqualStrings(
        "id: 0\nevent: chunk\ndata: {\"fleet_id\":\"z1\",\"kind\":\"chunk\",\"text\":\"hi\"}\n\n",
        out.items,
    );
}

test "writeTaggedFrame: an empty object gains the tag and stays valid JSON" {
    var out: std.ArrayList(u8) = .empty;
    defer out.deinit(testing.allocator);
    var w = sink(&out);

    try writeTaggedFrame(&w, 1, DEFAULT_KIND, "z1", "{}");
    try testing.expectEqualStrings(
        "id: 1\nevent: message\ndata: {\"fleet_id\":\"z1\"}\n\n",
        out.items,
    );
}

test "writeTaggedFrame: a non-object payload is refused, never emitted" {
    var out: std.ArrayList(u8) = .empty;
    defer out.deinit(testing.allocator);
    var w = sink(&out);

    // A publisher shape drift must not produce a frame at all — emitting a
    // half-spliced one would route garbage to a tile.
    try testing.expectError(error.NotAnObject, writeTaggedFrame(&w, 0, "chunk", "z1", "not json"));
    try testing.expectError(error.NotAnObject, writeTaggedFrame(&w, 0, "chunk", "z1", "["));
    try testing.expectError(error.NotAnObject, writeTaggedFrame(&w, 0, "chunk", "z1", ""));
    try testing.expectEqual(@as(usize, 0), out.items.len);
}
