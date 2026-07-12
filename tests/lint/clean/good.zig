//! Lint fixture: fully compliant — poisoned deinit + ownership phrase.
//! Produces zero findings inside or outside the roster.
const std = @import("std");

pub const Box = struct {
    alloc: std.mem.Allocator,
    buf: []u8,

    /// Caller must free the returned slice.
    pub fn dup(self: *Box, src: []const u8) ![]u8 {
        return self.alloc.dupe(u8, src);
    }

    pub fn deinit(self: *Box) void {
        self.alloc.free(self.buf);
        self.* = undefined;
    }
};
