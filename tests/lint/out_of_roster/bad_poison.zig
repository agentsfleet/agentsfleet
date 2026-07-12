//! Lint fixture: the SAME freeing-deinit-without-poison violation as
//! in_roster/bad_poison.zig, but placed outside every roster prefix. Here the
//! finding only WARNS (exit 0) — proving the roster scopes blocking vs advisory.
const std = @import("std");

pub const Thing = struct {
    alloc: std.mem.Allocator,
    buf: []u8,

    pub fn deinit(self: *Thing) void {
        self.alloc.free(self.buf);
    }
};
