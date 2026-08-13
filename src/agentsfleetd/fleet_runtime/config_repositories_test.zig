const std = @import("std");
const config_helpers = @import("config_helpers.zig");
const config_repositories = @import("config_repositories.zig");
const config_types = @import("config_types.zig");

const alloc = std.testing.allocator;

fn parseTrigger(src: []const u8) !config_types.FleetTrigger {
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, src, .{});
    defer parsed.deinit();
    return config_helpers.parseFleetTrigger(alloc, parsed.value.object);
}

fn parseBinding(src: []const u8, mode: config_repositories.ParseMode) !?config_types.RepositoryBinding {
    const parsed = try std.json.parseFromSlice(std.json.Value, alloc, src, .{});
    defer parsed.deinit();
    return config_repositories.parse(alloc, parsed.value.object, mode);
}

fn freeBinding(binding: config_types.RepositoryBinding) void {
    config_types.freeStringSlice(alloc, binding.repositories);
    if (binding.base_branch) |base| alloc.free(base);
}

fn repositoryTrigger(count: usize) ![]u8 {
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(alloc);
    try out.appendSlice(alloc, "{\"type\":\"webhook\",\"source\":\"github\",\"repositories\":[");
    for (0..count) |index| {
        if (index != 0) try out.append(alloc, ',');
        const repository = try std.fmt.allocPrint(alloc, "\"owner/repo-{d}\"", .{index});
        defer alloc.free(repository);
        try out.appendSlice(alloc, repository);
    }
    try out.appendSlice(alloc, "]}");
    return out.toOwnedSlice(alloc);
}

test "webhook repositories owns exact repository names and frees them" {
    const trigger = try parseTrigger(
        \\{"type":"webhook","source":"github","events":["pull_request"],"repositories":["agentsfleet/agentsfleet","indykish/oracle"]}
    );
    defer config_types.freeFleetTrigger(alloc, trigger);
    const repositories = trigger.webhook.repositories.?;
    try std.testing.expectEqual(@as(usize, 2), repositories.len);
    try std.testing.expectEqualStrings("agentsfleet/agentsfleet", repositories[0]);
    try std.testing.expectEqualStrings("indykish/oracle", repositories[1]);
}

test "webhook repositories is optional for the manual route" {
    const trigger = try parseTrigger(
        \\{"type":"webhook","source":"github","events":["workflow_run"]}
    );
    defer config_types.freeFleetTrigger(alloc, trigger);
    try std.testing.expect(trigger.webhook.repositories == null);
}

test "webhook repositories rejects empty, malformed, wrong-type, and oversized lists" {
    const invalid = [_][]const u8{
        \\{"type":"webhook","source":"github","repositories":[]}
        ,
        \\{"type":"webhook","source":"github","repositories":["owner"]}
        ,
        \\{"type":"webhook","source":"github","repositories":["owner/repo/extra"]}
        ,
        \\{"type":"webhook","source":"github","repositories":["owner/re po"]}
        ,
        \\{"type":"webhook","source":"github","repositories":[42]}
        ,
        \\{"type":"webhook","source":"github","repositories":"owner/repo"}
        ,
    };
    for (invalid) |src| try std.testing.expectError(config_types.FleetConfigError.InvalidFieldType, parseTrigger(src));

    const too_long = try std.fmt.allocPrint(alloc, "{{\"type\":\"webhook\",\"source\":\"github\",\"repositories\":[\"owner/{s}\"]}}", .{"r" ** 251});
    defer alloc.free(too_long);
    try std.testing.expectError(config_types.FleetConfigError.InvalidFieldType, parseTrigger(too_long));
}

test "webhook repositories accepts 64 entries and rejects 65" {
    const at_limit = try repositoryTrigger(64);
    defer alloc.free(at_limit);
    const trigger = try parseTrigger(at_limit);
    defer config_types.freeFleetTrigger(alloc, trigger);
    try std.testing.expectEqual(@as(usize, 64), trigger.webhook.repositories.?.len);

    const over_limit = try repositoryTrigger(65);
    defer alloc.free(over_limit);
    try std.testing.expectError(config_types.FleetConfigError.InvalidFieldType, parseTrigger(over_limit));
}

test "write repository binding requires and owns one trusted base branch" {
    try std.testing.expectError(
        config_types.FleetConfigError.MissingRequiredField,
        parseBinding("{\"repositories\":[\"acme/payments\"],\"repository_access\":\"write\"}", .authoring),
    );
    const maybe_binding = try parseBinding(
        "{\"repositories\":[\"acme/payments\"],\"repository_access\":\"write\",\"repository_base\":\"main\"}",
        .authoring,
    );
    const binding = maybe_binding orelse return error.TestUnexpectedResult;
    defer freeBinding(binding);
    try std.testing.expectEqual(config_types.RepositoryAccess.write, binding.access);
    try std.testing.expectEqualStrings("main", binding.base_branch.?);
}

test "read binding may omit a base and malformed write bases fail closed" {
    const maybe_read = try parseBinding(
        "{\"repositories\":[\"acme/payments\"],\"repository_access\":\"read\"}",
        .authoring,
    );
    const read = maybe_read orelse return error.TestUnexpectedResult;
    defer freeBinding(read);
    try std.testing.expect(read.base_branch == null);

    const invalid_bases = [_][]const u8{ "", "../main", "main..next", "main.lock", "main branch", "main@{1}" };
    for (invalid_bases) |base| {
        const src = try std.fmt.allocPrint(
            alloc,
            "{{\"repositories\":[\"acme/payments\"],\"repository_access\":\"write\",\"repository_base\":\"{s}\"}}",
            .{base},
        );
        defer alloc.free(src);
        try std.testing.expectError(config_types.FleetConfigError.InvalidFieldType, parseBinding(src, .authoring));
    }
}

test "stored write binding retains a missing base for upgrade refusal" {
    const source = "{\"repositories\":[\"acme/payments\"],\"repository_access\":\"write\"}";
    const maybe_binding = try parseBinding(source, .stored);
    const binding = maybe_binding orelse return error.TestUnexpectedResult;
    defer freeBinding(binding);
    try std.testing.expectEqual(config_types.RepositoryAccess.write, binding.access);
    try std.testing.expect(binding.base_branch == null);
}
