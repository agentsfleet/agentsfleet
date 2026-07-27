//! Boundary tests for the requirement ceilings.
//!
//! Every cap is asserted at the limit AND one past it. At-limit-passes is the
//! half that catches an off-by-one written as `>=`: a test that only proves a
//! huge input is rejected passes just as well against a cap that also rejects
//! every legitimate bundle.

const std = @import("std");
const limits = @import("requirement_limits.zig");

const testing = std.testing;

/// `n` names, each exactly `len` bytes, all aliasing one backing buffer.
///
/// Owns both allocations and frees them in `deinit`, rather than leaving the
/// caller to `defer` two frees in the right order: the element slices live
/// INSIDE the array being freed, so freeing the array first and then reading
/// `items[0]` to free the backing reads poisoned memory. That is not a
/// hypothetical — it is what the first version of this helper did, and it
/// segfaulted on `0xaa` bytes in every test here.
const Names = struct {
    items: []const []const u8,
    backing: []u8,
    alloc: std.mem.Allocator,

    fn deinit(self: Names) void {
        self.alloc.free(self.items);
        self.alloc.free(self.backing);
    }
};

fn names(alloc: std.mem.Allocator, n: usize, len: usize) !Names {
    const backing = try alloc.alloc(u8, len);
    errdefer alloc.free(backing);
    @memset(backing, 'a');
    const out = try alloc.alloc([]const u8, n);
    for (out) |*slot| slot.* = backing;
    return .{ .items = out, .backing = backing, .alloc = alloc };
}

const EMPTY: []const []const u8 = &.{};

test "requirement_limits: an empty bundle declares nothing and is accepted" {
    try limits.validateRequirements(EMPTY, EMPTY, EMPTY);
}

test "requirement_limits: each list is accepted exactly at its ceiling" {
    const alloc = testing.allocator;
    const creds = try names(alloc, limits.MAX_REQUIRED_CREDENTIALS, 1);
    defer creds.deinit();
    const tools = try names(alloc, limits.MAX_REQUIRED_TOOLS, 1);
    defer tools.deinit();
    const hosts = try names(alloc, limits.MAX_NETWORK_HOSTS, 1);
    defer hosts.deinit();

    try limits.validateRequirements(creds.items, tools.items, hosts.items);
}

test "requirement_limits: one credential past the ceiling is refused" {
    const alloc = testing.allocator;
    const creds = try names(alloc, limits.MAX_REQUIRED_CREDENTIALS + 1, 1);
    defer creds.deinit();

    try testing.expectError(
        limits.LimitError.TooManyCredentials,
        limits.validateRequirements(creds.items, EMPTY, EMPTY),
    );
}

test "requirement_limits: one tool past the ceiling is refused" {
    const alloc = testing.allocator;
    const tools = try names(alloc, limits.MAX_REQUIRED_TOOLS + 1, 1);
    defer tools.deinit();

    try testing.expectError(
        limits.LimitError.TooManyTools,
        limits.validateRequirements(EMPTY, tools.items, EMPTY),
    );
}

test "requirement_limits: one network host past the ceiling is refused" {
    const alloc = testing.allocator;
    const hosts = try names(alloc, limits.MAX_NETWORK_HOSTS + 1, 1);
    defer hosts.deinit();

    try testing.expectError(
        limits.LimitError.TooManyNetworkHosts,
        limits.validateRequirements(EMPTY, EMPTY, hosts.items),
    );
}

test "requirement_limits: a name is accepted at its length cap and refused past it" {
    const alloc = testing.allocator;
    const at = try names(alloc, 1, limits.MAX_REQUIREMENT_NAME_LEN);
    defer at.deinit();
    try limits.validateRequirements(at.items, EMPTY, EMPTY);

    const over = try names(alloc, 1, limits.MAX_REQUIREMENT_NAME_LEN + 1);
    defer over.deinit();
    try testing.expectError(
        limits.LimitError.RequirementNameTooLong,
        limits.validateRequirements(over.items, EMPTY, EMPTY),
    );
    // The same bound governs tools, not only credentials — the rule is about
    // what a requirement NAME may be, so a per-list copy could not drift.
    try testing.expectError(
        limits.LimitError.RequirementNameTooLong,
        limits.validateRequirements(EMPTY, over.items, EMPTY),
    );
}

test "requirement_limits: a host is accepted at the DNS maximum and refused past it" {
    const alloc = testing.allocator;
    const at = try names(alloc, 1, limits.MAX_NETWORK_HOST_LEN);
    defer at.deinit();
    try limits.validateRequirements(EMPTY, EMPTY, at.items);

    const over = try names(alloc, 1, limits.MAX_NETWORK_HOST_LEN + 1);
    defer over.deinit();
    try testing.expectError(
        limits.LimitError.NetworkHostTooLong,
        limits.validateRequirements(EMPTY, EMPTY, over.items),
    );
}

test "requirement_limits: a host longer than a name is still a valid host" {
    // Pins that the two length rules are genuinely separate. A host at 253 bytes
    // exceeds MAX_REQUIREMENT_NAME_LEN, so reusing one constant for both would
    // reject a resolvable domain.
    const alloc = testing.allocator;
    const host = try names(alloc, 1, limits.MAX_REQUIREMENT_NAME_LEN + 1);
    defer host.deinit();
    try testing.expect(limits.MAX_NETWORK_HOST_LEN > limits.MAX_REQUIREMENT_NAME_LEN);
    try limits.validateRequirements(EMPTY, EMPTY, host.items);
}

test "requirement_limits: the reasons map is bounded by entry count" {
    try limits.validateReasonCount(limits.MAX_REASON_ENTRIES);
    try testing.expectError(
        limits.LimitError.TooManyReasons,
        limits.validateReasonCount(limits.MAX_REASON_ENTRIES + 1),
    );
}

test "requirement_limits: one reason is bounded by key and by copy length" {
    const alloc = testing.allocator;

    const name_at = try names(alloc, 1, limits.MAX_REQUIREMENT_NAME_LEN);
    defer name_at.deinit();
    const reason_at = try names(alloc, 1, limits.MAX_REASON_LEN);
    defer reason_at.deinit();
    try limits.validateReason(name_at.items[0], reason_at.items[0]);

    const name_over = try names(alloc, 1, limits.MAX_REQUIREMENT_NAME_LEN + 1);
    defer name_over.deinit();
    try testing.expectError(
        limits.LimitError.RequirementNameTooLong,
        limits.validateReason(name_over.items[0], reason_at.items[0]),
    );

    const reason_over = try names(alloc, 1, limits.MAX_REASON_LEN + 1);
    defer reason_over.deinit();
    try testing.expectError(
        limits.LimitError.ReasonTooLong,
        limits.validateReason(name_at.items[0], reason_over.items[0]),
    );
}

test "requirement_limits: one reason per credential, so the two ceilings agree" {
    // Pin test: the reasons map exists to annotate declared credentials. If the
    // credential ceiling rose without this one, the gate could declare more
    // credentials than it can carry copy for.
    try testing.expectEqual(limits.MAX_REQUIRED_CREDENTIALS, limits.MAX_REASON_ENTRIES);
}
