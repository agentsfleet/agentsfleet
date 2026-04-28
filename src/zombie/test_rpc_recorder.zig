// Test-only RPC byte-stream recorder.
//
// Wraps the worker-side reader of executor frames by registering itself
// with `protocol.read_byte_recorder`; every payload read off the
// executor RPC socket is appended to a backing ArrayList. Tests then
// assert over the captured bytes — useful for invariants the decoded
// JSON would mask (e.g. a buggy encoder leaking a secret into a
// neighbouring field, or a placeholder failing to substitute).
//
// Single-writer by construction: install() asserts no recorder is
// already registered and uninstall() clears the slot. Tests must call
// uninstall() before exit, even on assertion failure (use defer).
//
// Pre-v2.0 teardown era: this fixture only attaches to the worker-side
// read path. Capturing bytes the executor *writes* would require a
// proxy socket, which is out of scope until we have a redaction-test
// driver against the production binary.

const std = @import("std");
const Allocator = std.mem.Allocator;

const protocol = @import("../executor/protocol.zig");

const RpcRecorder = @This();

alloc: Allocator,
captured: std.ArrayList(u8), // BUFFER GATE: ArrayList for captured — append-as-frames-arrive + indexOf for assertion-time grep.

pub fn init(alloc: Allocator) RpcRecorder {
    return .{ .alloc = alloc, .captured = .{} };
}

pub fn deinit(self: *RpcRecorder) void {
    self.captured.deinit(self.alloc);
}

/// Register this recorder with the protocol layer. Asserts no recorder
/// is currently installed — concurrent recorders would interleave and
/// produce useless captures.
pub fn install(self: *RpcRecorder) void {
    std.debug.assert(protocol.read_byte_recorder == null);
    protocol.read_byte_recorder = .{
        .alloc = self.alloc,
        .list = &self.captured,
    };
}

pub fn uninstall(self: *RpcRecorder) void {
    _ = self;
    protocol.read_byte_recorder = null;
}

/// True if `needle` appears anywhere in the captured byte stream.
pub fn contains(self: *const RpcRecorder, needle: []const u8) bool {
    return std.mem.indexOf(u8, self.captured.items, needle) != null;
}

/// Borrowed view of the captured bytes — valid until the next install/
/// drain or recorder deinit. Tests rarely need this; prefer `contains`.
pub fn bytes(self: *const RpcRecorder) []const u8 {
    return self.captured.items;
}

test "recorder captures payload bytes when installed" {
    const alloc = std.testing.allocator;
    var rec = RpcRecorder.init(alloc);
    defer rec.deinit();
    rec.install();
    defer rec.uninstall();

    // Manually push a payload through the same append path that
    // readFrameFromFd uses on each successful read.
    if (protocol.read_byte_recorder) |r| {
        try r.list.appendSlice(r.alloc, "hello world");
    }
    try std.testing.expect(rec.contains("hello"));
    try std.testing.expect(!rec.contains("absent"));
}
