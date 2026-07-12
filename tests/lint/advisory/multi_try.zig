//! Lint fixture: a multi-`try` init with no errdefer — the A2-ERRDEFER heuristic
//! emits an advisory warning (never blocks, even inside a roster prefix).
const std = @import("std");

pub const Pair = struct {
    a: []u8,
    b: []u8,

    pub fn init(alloc: std.mem.Allocator) !Pair {
        const a = try alloc.dupe(u8, "a");
        const b = try alloc.dupe(u8, "b");
        return .{ .a = a, .b = b };
    }

    pub fn deinit(self: *Pair, alloc: std.mem.Allocator) void {
        alloc.free(self.a);
        alloc.free(self.b);
        self.* = undefined;
    }
};
