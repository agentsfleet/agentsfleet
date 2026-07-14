//! Immediate erasure for owned byte buffers with known `u8` alignment.

const std = @import("std");

/// Erase `bytes` before returning its allocation to `alloc`.
pub fn freeBytes(alloc: std.mem.Allocator, bytes: []u8) void {
    if (bytes.len == 0) return;
    std.crypto.secureZero(u8, bytes);
    alloc.rawFree(bytes, .of(u8), @returnAddress());
}
