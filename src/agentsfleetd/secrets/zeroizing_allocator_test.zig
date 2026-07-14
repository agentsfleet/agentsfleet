const std = @import("std");
const ZeroizingAllocator = @import("zeroizing_allocator.zig");

const CONCURRENT_REQUESTS: usize = 100;
const CONCURRENCY_ROUNDS: usize = 3;
const REQUEST_LIFECYCLES: usize = 1000;
const ARENA_BACKING_SIZE: usize = 16 * 1024;
const COMPLEXITY_SIZES = [_]usize{ 256, 512, 1024, 2048 };

test "zeroizing allocator wipes complete allocation before free" {
    var backing: [128]u8 = undefined;
    var fixed = std.heap.FixedBufferAllocator.init(&backing);
    var zeroing = ZeroizingAllocator.wrap(fixed.allocator());
    const alloc = zeroing.allocator();

    const secret = try alloc.alloc(u8, 32);
    @memset(secret, 0xA5);
    const offset = @intFromPtr(secret.ptr) - @intFromPtr(backing[0..].ptr);
    alloc.free(secret);

    try std.testing.expectEqualSlices(u8, &([_]u8{0} ** 32), backing[offset..][0..32]);
}

test "zeroizing allocator preserves live allocation when shrink or remap falls back" {
    var backing: [256]u8 = undefined;
    var fixed = std.heap.FixedBufferAllocator.init(&backing);
    var zeroing = ZeroizingAllocator.wrap(fixed.allocator());
    const alloc = zeroing.allocator();

    const secret = try alloc.alloc(u8, 32);
    @memset(secret, 0x5A);
    try std.testing.expect(!alloc.resize(secret, 8));
    try std.testing.expect(alloc.rawRemap(secret, .of(u8), 8, @returnAddress()) == null);
    try std.testing.expectEqualSlices(u8, &([_]u8{0x5A} ** 32), secret);

    const old_offset = @intFromPtr(secret.ptr) - @intFromPtr(backing[0..].ptr);
    const shorter = try alloc.realloc(secret, 8);
    try std.testing.expectEqualSlices(u8, &([_]u8{0x5A} ** 8), shorter);
    try std.testing.expectEqualSlices(u8, &([_]u8{0} ** 32), backing[old_offset..][0..32]);
    const new_offset = @intFromPtr(shorter.ptr) - @intFromPtr(backing[0..].ptr);
    alloc.free(shorter);
    try std.testing.expectEqualSlices(u8, &([_]u8{0} ** 8), backing[new_offset..][0..8]);
}

test "dispatch arena releases only zeroed pages across allocation failures" {
    var reached_success = false;
    for (0..8) |fail_index| {
        var backing: [ARENA_BACKING_SIZE]u8 = undefined;
        @memset(&backing, 0);
        var fixed = std.heap.FixedBufferAllocator.init(&backing);
        var failing = std.testing.FailingAllocator.init(fixed.allocator(), .{ .fail_index = fail_index });
        const result = exerciseArena(failing.allocator());
        if (result) |_| {
            reached_success = true;
        } else |err| {
            try std.testing.expectEqual(error.OutOfMemory, err);
        }
        try std.testing.expect(allZero(&backing));
        if (reached_success) break;
    }
    try std.testing.expect(reached_success);
}

test "dispatch arena is leak-free at every allocation failure" {
    try std.testing.checkAllAllocationFailures(std.testing.allocator, exerciseArena, .{});
}

test "dispatch arena retains no storage across repeated request lifecycles" {
    var backing: [ARENA_BACKING_SIZE]u8 = undefined;
    @memset(&backing, 0);
    var fixed = std.heap.FixedBufferAllocator.init(&backing);

    for (0..REQUEST_LIFECYCLES) |_| {
        try exerciseArena(fixed.allocator());
        try std.testing.expectEqual(@as(usize, 0), fixed.end_index);
        try std.testing.expect(allZero(&backing));
    }
}

test "zeroizing free has constant allocator calls and linear byte work" {
    for (COMPLEXITY_SIZES) |size| {
        var backing: [ARENA_BACKING_SIZE]u8 = undefined;
        var fixed = std.heap.FixedBufferAllocator.init(&backing);
        var observer = FreeObserver{ .child = fixed.allocator() };
        var zeroing = ZeroizingAllocator.wrap(observer.allocator());
        const alloc = zeroing.allocator();

        const secret = try alloc.alloc(u8, size);
        @memset(secret, 0xA5);
        alloc.free(secret);

        try std.testing.expectEqual(@as(usize, 1), observer.alloc_calls);
        try std.testing.expectEqual(@as(usize, 1), observer.free_calls);
        try std.testing.expectEqual(size, observer.freed_bytes);
        try std.testing.expect(observer.frees_were_zero);
    }
}

test "zeroizing request arenas share one allocator across 100 concurrent requests" {
    var debug = std.heap.DebugAllocator(.{ .thread_safe = true }){};
    defer std.testing.expectEqual(.ok, debug.deinit()) catch @panic("concurrent allocator leaked");

    for (0..CONCURRENCY_ROUNDS) |_| {
        var run = ConcurrentRun{ .child = debug.allocator() };
        var threads: [CONCURRENT_REQUESTS]std.Thread = undefined;
        for (&threads) |*thread| thread.* = try std.Thread.spawn(.{}, ConcurrentRun.worker, .{&run});
        while (run.ready.load(.acquire) != CONCURRENT_REQUESTS) std.atomic.spinLoopHint();
        run.start.store(true, .release);
        for (threads) |thread| thread.join();
        try std.testing.expectEqual(@as(u32, CONCURRENT_REQUESTS), run.completed.load(.acquire));
        try std.testing.expectEqual(@as(u32, 0), run.failures.load(.acquire));
    }
}

fn exerciseArena(child: std.mem.Allocator) !void {
    var zeroing = ZeroizingAllocator.wrap(child);
    var arena = std.heap.ArenaAllocator.init(zeroing.allocator());
    defer arena.deinit();
    const alloc = arena.allocator();
    const first = try alloc.alloc(u8, 257);
    @memset(first, 0xC3);
    const second = try alloc.alloc(u8, 1025);
    @memset(second, 0xD4);
}

fn allZero(bytes: []const u8) bool {
    for (bytes) |byte| if (byte != 0) return false;
    return true;
}

const FreeObserver = struct {
    child: std.mem.Allocator,
    alloc_calls: usize = 0,
    free_calls: usize = 0,
    freed_bytes: usize = 0,
    frees_were_zero: bool = true,

    fn allocator(self: *FreeObserver) std.mem.Allocator {
        return .{ .ptr = self, .vtable = &vtable };
    }

    const vtable: std.mem.Allocator.VTable = .{
        .alloc = allocate,
        .resize = resize,
        .remap = remap,
        .free = free,
    };

    fn allocate(ctx: *anyopaque, len: usize, alignment: std.mem.Alignment, ret_addr: usize) ?[*]u8 {
        const self: *FreeObserver = @ptrCast(@alignCast(ctx));
        self.alloc_calls += 1;
        return self.child.rawAlloc(len, alignment, ret_addr);
    }

    fn resize(ctx: *anyopaque, memory: []u8, alignment: std.mem.Alignment, new_len: usize, ret_addr: usize) bool {
        const self: *FreeObserver = @ptrCast(@alignCast(ctx));
        return self.child.rawResize(memory, alignment, new_len, ret_addr);
    }

    fn remap(ctx: *anyopaque, memory: []u8, alignment: std.mem.Alignment, new_len: usize, ret_addr: usize) ?[*]u8 {
        const self: *FreeObserver = @ptrCast(@alignCast(ctx));
        return self.child.rawRemap(memory, alignment, new_len, ret_addr);
    }

    fn free(ctx: *anyopaque, memory: []u8, alignment: std.mem.Alignment, ret_addr: usize) void {
        const self: *FreeObserver = @ptrCast(@alignCast(ctx));
        self.free_calls += 1;
        self.freed_bytes += memory.len;
        self.frees_were_zero = self.frees_were_zero and allZero(memory);
        self.child.rawFree(memory, alignment, ret_addr);
    }
};

const ConcurrentRun = struct {
    child: std.mem.Allocator,
    ready: std.atomic.Value(u32) = .init(0),
    start: std.atomic.Value(bool) = .init(false),
    completed: std.atomic.Value(u32) = .init(0),
    failures: std.atomic.Value(u32) = .init(0),

    fn worker(self: *ConcurrentRun) void {
        _ = self.ready.fetchAdd(1, .release);
        while (!self.start.load(.acquire)) std.atomic.spinLoopHint();
        exerciseArena(self.child) catch {
            _ = self.failures.fetchAdd(1, .release);
            return;
        };
        _ = self.completed.fetchAdd(1, .release);
    }
};
