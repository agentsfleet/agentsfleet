//! Build the substituted request argument map for the policy HTTP tool.

const std = @import("std");
const nullclaw = @import("nullclaw");
const JsonObjectMap = nullclaw.tools.JsonObjectMap;
const credential_request = @import("../credential_request.zig");
const secret_substitution = @import("secret_substitution.zig");

pub const ARG_BODY = "body";
pub const ARG_HEADERS = "headers";
pub const ARG_METHOD = "method";
pub const ARG_URL = "url";

pub fn build(
    arena: std.mem.Allocator,
    args: JsonObjectMap,
    substituted_url: []const u8,
    secrets_map: ?std.json.Value,
    resolver: *credential_request.MintResolver,
) error{ SubstFailed, Leftover, OutOfMemory }!JsonObjectMap {
    var substituted: JsonObjectMap = .empty;
    try substituted.put(arena, ARG_URL, .{ .string = substituted_url });
    if (args.get(ARG_METHOD)) |method| {
        try substituted.put(arena, ARG_METHOD, method);
    }

    if (args.get(ARG_HEADERS)) |headers| {
        if (headers == .object) {
            var substituted_headers: JsonObjectMap = .empty;
            var it = headers.object.iterator();
            while (it.next()) |entry| {
                const value = try substituteValue(arena, entry.value_ptr.*, secrets_map, resolver);
                try substituted_headers.put(arena, entry.key_ptr.*, value);
            }
            try substituted.put(arena, ARG_HEADERS, .{ .object = substituted_headers });
        } else {
            try substituted.put(arena, ARG_HEADERS, headers);
        }
    }

    if (args.get(ARG_BODY)) |body| {
        const value = try substituteValue(arena, body, secrets_map, resolver);
        try substituted.put(arena, ARG_BODY, value);
    }
    return substituted;
}

fn substituteValue(
    arena: std.mem.Allocator,
    value: std.json.Value,
    secrets_map: ?std.json.Value,
    resolver: *credential_request.MintResolver,
) error{ SubstFailed, Leftover }!std.json.Value {
    return switch (value) {
        .string => |raw| blk: {
            const replaced = secret_substitution.substitute(arena, raw, secrets_map, resolver) catch
                return error.SubstFailed;
            if (!secret_substitution.assertNoLeftover(replaced)) return error.Leftover;
            break :blk .{ .string = replaced };
        },
        else => value,
    };
}

test "request argument builder closes every allocation failure path" {
    const Case = struct {
        fn run(backing: std.mem.Allocator) !void {
            var arena_state = std.heap.ArenaAllocator.init(backing);
            defer arena_state.deinit();
            const arena = arena_state.allocator();

            var headers: JsonObjectMap = .empty;
            try headers.put(arena, "X-Test", .{ .integer = 7 });
            var args: JsonObjectMap = .empty;
            try args.put(arena, ARG_METHOD, .{ .string = "POST" });
            try args.put(arena, ARG_HEADERS, .{ .object = headers });
            try args.put(arena, ARG_BODY, .{ .bool = true });
            var resolver = credential_request.MintResolver{ .mintable = &.{}, .channel = null };

            const substituted = try build(arena, args, "https://api.github.com/repos/o/r/pulls", null, &resolver);
            try std.testing.expectEqualStrings("POST", substituted.get(ARG_METHOD).?.string);
            try std.testing.expect(substituted.get(ARG_HEADERS).? == .object);
            try std.testing.expect(substituted.get(ARG_BODY).?.bool);
        }
    };
    try std.testing.checkAllAllocationFailures(std.testing.allocator, Case.run, .{});
}
