//! The one JSON spelling of a repository binding: what the park records onto
//! the gate row's `stated_binding`, and what the write mint compares against.
//!
//! Serializer and matcher live together because they are two halves of one
//! wire shape — a drift between them would let a mint refuse the very binding
//! its own park recorded. Repository comparison is case-insensitive and
//! order-insensitive, mirroring `credentials/integration_github_reach.zig`
//! (GitHub owners and names compare that way); access compares exactly.

const std = @import("std");
const Allocator = std.mem.Allocator;
const config_types = @import("config_types.zig");

const F_REPOSITORIES = "repositories";
const F_ACCESS = "access";

/// Serialize a binding as `{"repositories":[…],"access":"read|write"}`.
/// Caller must free. Repository spellings pass through verbatim — the matcher
/// owns the case-insensitivity, not the record.
pub fn serialize(alloc: Allocator, binding: config_types.RepositoryBinding) ![]u8 {
    var aw: std.Io.Writer.Allocating = .init(alloc);
    errdefer aw.deinit();
    const w = &aw.writer;
    try w.writeAll("{\"" ++ F_REPOSITORIES ++ "\":[");
    for (binding.repositories, 0..) |repo, i| {
        if (i > 0) try w.writeByte(',');
        try w.writeByte('"');
        try writeJsonEscaped(w, repo);
        try w.writeByte('"');
    }
    try w.writeAll("],\"" ++ F_ACCESS ++ "\":\"");
    try w.writeAll(switch (binding.access) {
        .read => config_types.S_REPOSITORY_ACCESS_READ,
        .write => config_types.S_REPOSITORY_ACCESS_WRITE,
    });
    try w.writeAll("\"}");
    return aw.toOwnedSlice();
}

/// Does `stated_json` describe exactly `binding`? Same access, same repository
/// SET (case-insensitive, order-insensitive, both directions — a stated
/// binding that is narrower OR wider than the current one is a mismatch).
/// Malformed JSON is a mismatch, never a pass: unknown reach must not be the
/// permissive branch.
pub fn matches(alloc: Allocator, stated_json: []const u8, binding: config_types.RepositoryBinding) bool {
    var parsed = std.json.parseFromSlice(std.json.Value, alloc, stated_json, .{}) catch return false;
    defer parsed.deinit();
    const obj = switch (parsed.value) {
        .object => |o| o,
        else => return false,
    };

    const access = switch (obj.get(F_ACCESS) orelse return false) {
        .string => |s| config_types.RepositoryAccess.fromSlice(s) orelse return false,
        else => return false,
    };
    if (access != binding.access) return false;

    const stated = switch (obj.get(F_REPOSITORIES) orelse return false) {
        .array => |a| a.items,
        else => return false,
    };
    // Direction one: everything stated is still declared.
    for (stated) |entry| {
        const name = switch (entry) {
            .string => |s| s,
            else => return false,
        };
        if (!containsIgnoreCase(binding.repositories, name)) return false;
    }
    // Direction two: everything declared was stated — a config that GREW a
    // repository since the approval is the drift this check exists to catch.
    for (binding.repositories) |want| {
        if (!statedContains(stated, want)) return false;
    }
    return true;
}

fn containsIgnoreCase(haystack: []const []const u8, needle: []const u8) bool {
    for (haystack) |candidate| {
        if (std.ascii.eqlIgnoreCase(candidate, needle)) return true;
    }
    return false;
}

fn statedContains(stated: []const std.json.Value, want: []const u8) bool {
    for (stated) |entry| {
        const name = switch (entry) {
            .string => |s| s,
            else => continue,
        };
        if (std.ascii.eqlIgnoreCase(name, want)) return true;
    }
    return false;
}

fn writeJsonEscaped(w: anytype, s: []const u8) !void {
    for (s) |c| {
        switch (c) {
            '"' => try w.writeAll("\\\""),
            '\\' => try w.writeAll("\\\\"),
            '\n' => try w.writeAll("\\n"),
            '\r' => try w.writeAll("\\r"),
            '\t' => try w.writeAll("\\t"),
            else => try w.writeByte(c),
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

const testing = std.testing;

const REPO_PAYMENTS = "acme/payments";
const REPOS_ONE = [_][]const u8{REPO_PAYMENTS};
const REPOS_TWO = [_][]const u8{ REPO_PAYMENTS, "acme/widgets" };

test "binding json: serialize round-trips through matches" {
    const binding: config_types.RepositoryBinding = .{ .repositories = &REPOS_ONE, .access = .write };
    const json = try serialize(testing.allocator, binding);
    defer testing.allocator.free(json);
    try testing.expect(matches(testing.allocator, json, binding));
    // pin test: literal is the contract
    try testing.expectEqualStrings("{\"repositories\":[\"acme/payments\"],\"access\":\"write\"}", json);
}

test "binding json: access drift and repository drift both refuse" {
    // pin test: literal is the contract
    const stated = "{\"repositories\":[\"acme/payments\"],\"access\":\"write\"}";
    // Access narrowed since approval.
    try testing.expect(!matches(testing.allocator, stated, .{ .repositories = &REPOS_ONE, .access = .read }));
    // Config GREW a repository the human never saw on the card.
    try testing.expect(!matches(testing.allocator, stated, .{ .repositories = &REPOS_TWO, .access = .write }));
    // Config now names a different repository entirely.
    const other = [_][]const u8{"acme/other"};
    try testing.expect(!matches(testing.allocator, stated, .{ .repositories = &other, .access = .write }));
}

test "binding json: comparison is case-insensitive and order-insensitive" {
    const stated = "{\"repositories\":[\"Acme/Widgets\",\"acme/payments\"],\"access\":\"write\"}";
    const declared = [_][]const u8{ "acme/payments", "acme/widgets" };
    try testing.expect(matches(testing.allocator, stated, .{ .repositories = &declared, .access = .write }));
}

test "binding json: malformed stated json is a mismatch, never a pass" {
    const binding: config_types.RepositoryBinding = .{ .repositories = &REPOS_ONE, .access = .write };
    try testing.expect(!matches(testing.allocator, "not json", binding));
    try testing.expect(!matches(testing.allocator, "{}", binding));
    try testing.expect(!matches(testing.allocator, "{\"repositories\":\"all\",\"access\":\"write\"}", binding));
    try testing.expect(!matches(testing.allocator, "{\"repositories\":[42],\"access\":\"write\"}", binding));
}
