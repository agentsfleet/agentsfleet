//! Tolerant lease decoder for rolling daemon and runner updates.

const std = @import("std");
const protocol = @import("contract").protocol;

pub fn parse(
    alloc: std.mem.Allocator,
    body: []const u8,
) std.json.ParseError(std.json.Scanner)!std.json.Parsed(protocol.LeaseResponse) {
    return std.json.parseFromSlice(protocol.LeaseResponse, alloc, body, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
    });
}
