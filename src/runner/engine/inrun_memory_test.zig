//! Tests for `inrun_memory.zig`. A sibling file, not inline blocks: the scripted
//! `Memory` below is test support, and test support living in a product file is
//! counted as product by the coverage denominator — where its unexercised
//! vtable stubs read as permanently dark shipped code.

const std = @import("std");
const nullclaw = @import("nullclaw");
const clock = @import("common").clock;
const protocol = @import("contract").protocol;
const pipe_proto = @import("../pipe_proto.zig");

const memory_mod = nullclaw.memory;
const Memory = memory_mod.Memory;
const MemoryCategory = memory_mod.MemoryCategory;

const inrun_memory = @import("inrun_memory.zig");
const initRuntime = inrun_memory.initRuntime;
const seed = inrun_memory.seed;
const MemoryCapturer = inrun_memory.MemoryCapturer;

test "initRuntime builds a usable file-less store; seed + capture round-trip" {
    const alloc = std.testing.allocator;
    var rt = initRuntime(alloc, "/tmp") orelse return error.SkipZigTest; // sqlite disabled in some builds
    defer rt.deinit();

    // Seed two real entries plus an internal bootstrap key that must be filtered.
    seed(rt.memory, &.{
        .{ .key = "deploy_target", .content = "fly", .category = "core" },
        .{ .key = "owner", .content = "indy", .category = "core" },
    });
    rt.memory.store("__bootstrap.prompt.AGENTS.md", "noise", .core, null) catch {};

    const fds = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(fds[0]);
    var cap = MemoryCapturer{ .mem = rt.memory, .fd = fds[1], .alloc = alloc };
    cap.capture();
    pipe_proto.testOsClose(fds[1]);

    const dl = clock.nowMillis() + 5_000;
    const out = try pipe_proto.readFrame(alloc, fds[0], dl, 1 << 20);
    try std.testing.expect(out == .frame);
    defer alloc.free(out.frame.payload);
    try std.testing.expectEqual(pipe_proto.FrameType.memory, out.frame.ftype);

    const parsed = try std.json.parseFromSlice([]protocol.MemoryDelta, alloc, out.frame.payload, .{});
    defer parsed.deinit();
    // The two real entries survive; the bootstrap key is filtered out.
    try std.testing.expectEqual(@as(usize, 2), parsed.value.len);
}

// ---------------------------------------------------------------------------
// Both halves of this module are best-effort by contract: recall degrades, it
// never blocks a run. That promise lives entirely in arms the happy path above
// cannot reach, so each one is driven here against a store scripted to fail.
// ---------------------------------------------------------------------------

/// A `Memory` whose chosen operation always fails. Everything else answers
/// benignly — the point is one scripted failure, not a second source of noise.
const FailingMemory = struct {
    fail: enum { store, list },

    const vtable = memory_mod.Memory.VTable{
        .name = name,
        .store = store,
        .recall = emptyList,
        .get = getNone,
        .list = list,
        .forget = forgetNone,
        .count = countZero,
        .healthCheck = healthy,
        .deinit = noop,
    };

    fn memory(self: *FailingMemory) Memory {
        return .{ .ptr = self, .vtable = &vtable };
    }

    fn me(ptr: *anyopaque) *FailingMemory {
        return @ptrCast(@alignCast(ptr));
    }

    fn name(_: *anyopaque) []const u8 {
        return "failing";
    }

    fn store(ptr: *anyopaque, _: []const u8, _: []const u8, _: MemoryCategory, _: ?[]const u8) anyerror!void {
        if (me(ptr).fail == .store) return error.StoreRefused;
    }

    fn list(ptr: *anyopaque, alloc: std.mem.Allocator, _: ?MemoryCategory, _: ?[]const u8) anyerror![]memory_mod.MemoryEntry {
        if (me(ptr).fail == .list) return error.ListRefused;
        return alloc.alloc(memory_mod.MemoryEntry, 0);
    }

    fn emptyList(_: *anyopaque, alloc: std.mem.Allocator, _: []const u8, _: usize, _: ?[]const u8) anyerror![]memory_mod.MemoryEntry {
        return alloc.alloc(memory_mod.MemoryEntry, 0);
    }

    fn getNone(_: *anyopaque, _: std.mem.Allocator, _: []const u8) anyerror!?memory_mod.MemoryEntry {
        return null;
    }

    fn forgetNone(_: *anyopaque, _: []const u8) anyerror!bool {
        return false;
    }

    fn countZero(_: *anyopaque) anyerror!usize {
        return 0;
    }

    fn healthy(_: *anyopaque) bool {
        return true;
    }

    fn noop(_: *anyopaque) void {}
};

test "a store that refuses every entry is skipped per entry, never fatal" {
    // The parent hydrates prior memory before the fleet starts. One bad row must
    // not abort the run before any work happens — seed returns normally and the
    // fleet proceeds with less recall, which is the whole degrade-never-block
    // contract in one call.
    var failing = FailingMemory{ .fail = .store };
    seed(failing.memory(), &.{
        .{ .key = "deploy_target", .content = "fly", .category = "core" },
        .{ .key = "owner", .content = "indy", .category = "core" },
    });
}

test "a capture whose enumeration fails writes no frame at all" {
    // Half a frame is worse than none: the parent forwards whatever arrives
    // straight to the memory endpoint, so a failed list must return before the
    // write rather than push an empty set that would read as "the fleet
    // remembered nothing".
    var failing = FailingMemory{ .fail = .list };
    const fds = try pipe_proto.testOsPipe();
    defer pipe_proto.testOsClose(fds[0]);
    defer pipe_proto.testOsClose(fds[1]);

    const cap = MemoryCapturer{ .mem = failing.memory(), .fd = fds[1], .alloc = std.testing.allocator };
    cap.capture();

    // Nothing was written, so a read with an already-expired deadline finds no
    // frame rather than blocking on one that is never coming.
    const out = pipe_proto.readFrame(std.testing.allocator, fds[0], clock.nowMillis() - 1, 1 << 20) catch return;
    if (out == .frame) {
        std.testing.allocator.free(out.frame.payload);
        return error.TestUnexpectedResult;
    }
}

test "a capture whose write fails is logged and still returns to the run" {
    const alloc = std.testing.allocator;
    var rt = initRuntime(alloc, "/tmp") orelse return error.SkipZigTest;
    defer rt.deinit();
    seed(rt.memory, &.{.{ .key = "owner", .content = "indy", .category = "core" }});

    // The read end is gone, so the write fails at the transport. A checkpoint
    // losing its frame is a logged blip, never a failed run — the durable record
    // is the next checkpoint or run end.
    const fds = try pipe_proto.testOsPipe();
    pipe_proto.testOsClose(fds[0]);
    defer pipe_proto.testOsClose(fds[1]);
    const cap = MemoryCapturer{ .mem = rt.memory, .fd = fds[1], .alloc = alloc };
    cap.capture();
}
