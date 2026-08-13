//! GitHub repository bindings translated into provider-neutral lease rules.

const std = @import("std");
const config_types = @import("../fleet_runtime/config_types.zig");
const execution_policy = @import("contract").execution_policy;

const API_HOST = "api.github.com";
const CREDENTIAL_GITHUB = "github";
const REFS_HEADS_PREFIX = "refs/heads/";
const TEST_BASE_BRANCH = "main";
const TEST_REPAIR_BRANCH = "agentsfleet-repair/run-123";
const TEST_REPOSITORY = "acme/payments";

/// Every returned slice is owned by `arena`; callers release the arena once the
/// lease response has serialized.
pub fn build(
    arena: std.mem.Allocator,
    binding: config_types.RepositoryBinding,
    repair_branch: ?[]const u8,
) ![]const execution_policy.HttpOriginPolicy {
    var rules: std.ArrayList(execution_policy.HttpRequestRule) = .empty;
    for (binding.repositories) |repository| try appendReadRules(arena, &rules, repository);
    if (binding.access == .write) {
        const branch = repair_branch orelse return error.MissingRepairBranch;
        const base = binding.base_branch orelse return error.MissingRepositoryBase;
        if (binding.repositories.len != 1) return error.InvalidRepositoryCount;
        try appendWriteRules(arena, &rules, binding.repositories[0], branch, base);
    }
    const origins = try arena.alloc(execution_policy.HttpOriginPolicy, 1);
    origins[0] = .{
        .host = API_HOST,
        .credential_names = &.{CREDENTIAL_GITHUB},
        .requests = try rules.toOwnedSlice(arena),
    };
    return origins;
}

fn appendReadRules(
    arena: std.mem.Allocator,
    rules: *std.ArrayList(execution_policy.HttpRequestRule),
    repository: []const u8,
) !void {
    const prefix = try std.fmt.allocPrint(arena, "/repos/{s}/", .{repository});
    try rules.append(arena, .{ .method = .get, .path = prefix, .path_match = .prefix });
    try rules.append(arena, .{ .method = .head, .path = prefix, .path_match = .prefix });
}

fn appendWriteRules(
    arena: std.mem.Allocator,
    rules: *std.ArrayList(execution_policy.HttpRequestRule),
    repository: []const u8,
    branch: []const u8,
    base: []const u8,
) !void {
    try appendOpenPost(arena, rules, repository, "/git/blobs");
    try appendOpenPost(arena, rules, repository, "/git/trees");
    try appendOpenPost(arena, rules, repository, "/git/commits");
    try appendLockedPost(arena, rules, repository, "/git/refs", &.{.{
        .name = "ref",
        .string_value = try std.fmt.allocPrint(arena, "{s}{s}", .{ REFS_HEADS_PREFIX, branch }),
    }});
    try appendLockedPost(arena, rules, repository, "/pulls", &.{
        .{ .name = "head", .string_value = branch },
        .{ .name = "base", .string_value = base },
        .{ .name = "draft", .boolean_value = true },
    });
}

fn appendOpenPost(
    arena: std.mem.Allocator,
    rules: *std.ArrayList(execution_policy.HttpRequestRule),
    repository: []const u8,
    suffix: []const u8,
) !void {
    try rules.append(arena, .{
        .method = .post,
        .path = try std.fmt.allocPrint(arena, "/repos/{s}{s}", .{ repository, suffix }),
    });
}

fn appendLockedPost(
    arena: std.mem.Allocator,
    rules: *std.ArrayList(execution_policy.HttpRequestRule),
    repository: []const u8,
    suffix: []const u8,
    fields: []const execution_policy.HttpJsonFieldRule,
) !void {
    try rules.append(arena, .{
        .method = .post,
        .path = try std.fmt.allocPrint(arena, "/repos/{s}{s}", .{ repository, suffix }),
        .json_fields = try arena.dupe(execution_policy.HttpJsonFieldRule, fields),
    });
}

test "write binding authors exact generic ref and pull request rules" {
    var arena_state = std.heap.ArenaAllocator.init(std.testing.allocator);
    defer arena_state.deinit();
    const repositories = [_][]const u8{TEST_REPOSITORY};
    const origins = try build(arena_state.allocator(), .{
        .repositories = &repositories,
        .access = .write,
        .base_branch = TEST_BASE_BRANCH,
    }, TEST_REPAIR_BRANCH);
    try std.testing.expectEqual(@as(usize, 1), origins.len);
    try std.testing.expectEqualStrings(API_HOST, origins[0].host);
    try std.testing.expectEqual(@as(usize, 7), origins[0].requests.len);
    const pull = origins[0].requests[6];
    try std.testing.expectEqualStrings("/repos/acme/payments/pulls", pull.path);
    try std.testing.expectEqualStrings(TEST_BASE_BRANCH, pull.json_fields[1].string_value.?);
    try std.testing.expect(pull.json_fields[2].boolean_value.?);
}

test "repository HTTP policy closes every allocation failure path" {
    const Case = struct {
        fn run(backing: std.mem.Allocator) !void {
            var arena_state = std.heap.ArenaAllocator.init(backing);
            defer arena_state.deinit();
            const repositories = [_][]const u8{TEST_REPOSITORY};
            const origins = try build(arena_state.allocator(), .{
                .repositories = &repositories,
                .access = .write,
                .base_branch = TEST_BASE_BRANCH,
            }, TEST_REPAIR_BRANCH);
            try std.testing.expectEqualStrings(API_HOST, origins[0].host);
            try std.testing.expectEqual(@as(usize, 7), origins[0].requests.len);
        }
    };
    try std.testing.checkAllAllocationFailures(std.testing.allocator, Case.run, .{});
}
