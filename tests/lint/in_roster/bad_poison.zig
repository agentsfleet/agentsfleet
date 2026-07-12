//! Lint fixture: a freeing deinit that omits the A5 `self.* = undefined` poison.
//! Inside a roster prefix this is a BLOCKING A5-POISON violation.
const std = @import("std");

pub const Thing = struct {
    alloc: std.mem.Allocator,
    buf: []u8,

    pub fn deinit(self: *Thing) void {
        self.alloc.free(self.buf);
    }
};
