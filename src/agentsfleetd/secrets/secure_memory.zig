//! Immediate erasure for owned byte buffers with known `u8` alignment.

const std = @import("std");

/// Erase `bytes` before returning its allocation to `alloc`.
pub fn freeBytes(alloc: std.mem.Allocator, bytes: []u8) void {
    if (bytes.len == 0) return;
    std.crypto.secureZero(u8, bytes);
    alloc.rawFree(bytes, .of(u8), @returnAddress());
}

test "freeBytes zeroes the buffer before handing it to the allocator" {
    // Spy allocator: its free hook observes the bytes the moment they come
    // back — the ONLY legal point to prove the scrub happened.
    const Spy = struct {
        parent: std.mem.Allocator,
        freed_all_zero: ?bool = null,

        fn allocFn(ctx: *anyopaque, len: usize, alignment: std.mem.Alignment, ra: usize) ?[*]u8 {
            const self: *@This() = @ptrCast(@alignCast(ctx));
            return self.parent.vtable.alloc(self.parent.ptr, len, alignment, ra);
        }
        fn resizeFn(ctx: *anyopaque, memory: []u8, alignment: std.mem.Alignment, new_len: usize, ra: usize) bool {
            const self: *@This() = @ptrCast(@alignCast(ctx));
            return self.parent.vtable.resize(self.parent.ptr, memory, alignment, new_len, ra);
        }
        fn remapFn(ctx: *anyopaque, memory: []u8, alignment: std.mem.Alignment, new_len: usize, ra: usize) ?[*]u8 {
            const self: *@This() = @ptrCast(@alignCast(ctx));
            return self.parent.vtable.remap(self.parent.ptr, memory, alignment, new_len, ra);
        }
        fn freeFn(ctx: *anyopaque, memory: []u8, alignment: std.mem.Alignment, ra: usize) void {
            const self: *@This() = @ptrCast(@alignCast(ctx));
            self.freed_all_zero = std.mem.allEqual(u8, memory, 0);
            self.parent.vtable.free(self.parent.ptr, memory, alignment, ra);
        }
        fn allocator(self: *@This()) std.mem.Allocator {
            return .{ .ptr = self, .vtable = &.{
                .alloc = allocFn,
                .resize = resizeFn,
                .remap = remapFn,
                .free = freeFn,
            } };
        }
    };
    var spy = Spy{ .parent = std.testing.allocator };
    const a = spy.allocator();
    const buf = try a.alloc(u8, 32);
    @memset(buf, 0xAA);
    freeBytes(a, buf);
    try std.testing.expect(spy.freed_all_zero.?);
}
