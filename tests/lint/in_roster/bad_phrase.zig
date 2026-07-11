//! Lint fixture: an owned-slice-returning pub fn with no ownership phrase.
//! Inside a roster prefix this is a BLOCKING A5-PHRASE violation.
const std = @import("std");

pub fn duplicate(alloc: std.mem.Allocator, src: []const u8) ![]u8 {
    return alloc.dupe(u8, src);
}
