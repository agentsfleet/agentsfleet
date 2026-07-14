//! Allocator wrapper that erases released storage before returning it.
//!
//! The wrapper borrows its child allocator. It deliberately refuses remaps and
//! in-place shrink requests: callers fall back to allocate-copy-free, which
//! keeps live allocations intact until the old allocation reaches `free`.

const std = @import("std");
const Allocator = std.mem.Allocator;
const Self = @This();

child: Allocator,

pub fn wrap(child: Allocator) Self {
    return .{ .child = child };
}

pub fn allocator(self: *Self) Allocator {
    return .{ .ptr = self, .vtable = &vtable };
}

const vtable: Allocator.VTable = .{
    .alloc = allocate,
    .resize = resize,
    .remap = remap,
    .free = free,
};

fn allocate(ctx: *anyopaque, len: usize, alignment: std.mem.Alignment, ret_addr: usize) ?[*]u8 {
    const self: *Self = @ptrCast(@alignCast(ctx));
    return self.child.rawAlloc(len, alignment, ret_addr);
}

fn resize(ctx: *anyopaque, memory: []u8, alignment: std.mem.Alignment, new_len: usize, ret_addr: usize) bool {
    const self: *Self = @ptrCast(@alignCast(ctx));
    if (new_len < memory.len) return false;
    if (new_len == memory.len) return true;
    return self.child.rawResize(memory, alignment, new_len, ret_addr);
}

fn remap(ctx: *anyopaque, memory: []u8, alignment: std.mem.Alignment, new_len: usize, ret_addr: usize) ?[*]u8 {
    _ = ctx;
    _ = memory;
    _ = alignment;
    _ = new_len;
    _ = ret_addr;
    return null;
}

fn free(ctx: *anyopaque, memory: []u8, alignment: std.mem.Alignment, ret_addr: usize) void {
    const self: *Self = @ptrCast(@alignCast(ctx));
    std.crypto.secureZero(u8, memory);
    self.child.rawFree(memory, alignment, ret_addr);
}
