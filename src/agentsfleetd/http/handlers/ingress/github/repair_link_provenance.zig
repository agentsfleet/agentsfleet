//! GitHub-payload provenance checks for repair Pull Requests.

const std = @import("std");

const FIELD_PULL_REQUEST = "pull_request";
const FIELD_REPOSITORY = "repository";
const FIELD_FULL_NAME = "full_name";
const FIELD_USER = "user";
const FIELD_LOGIN = "login";
const FIELD_HEAD = "head";
const FIELD_BASE = "base";
const FIELD_REF = "ref";
const FIELD_REPO = "repo";
const FIELD_FORK = "fork";
const BOT_SUFFIX = "[bot]";

/// Require the configured App bot, approved base, same-repository sides, and a non-fork head.
pub fn ownedPullRequest(root: std.json.ObjectMap, repository: []const u8, base_branch: []const u8, app_slug: []const u8) bool {
    if (app_slug.len == 0) return false;
    const payload_repo = objectField(root, FIELD_REPOSITORY) orelse return false;
    if (!equalName(stringField(payload_repo, FIELD_FULL_NAME), repository)) return false;
    const pull_request = objectField(root, FIELD_PULL_REQUEST) orelse return false;
    const user = objectField(pull_request, FIELD_USER) orelse return false;
    if (!isAppBot(stringField(user, FIELD_LOGIN) orelse return false, app_slug)) return false;
    const head = objectField(pull_request, FIELD_HEAD) orelse return false;
    const base = objectField(pull_request, FIELD_BASE) orelse return false;
    if (!std.mem.eql(u8, stringField(base, FIELD_REF) orelse return false, base_branch)) return false;
    return sameRepository(head, repository, true) and sameRepository(base, repository, false);
}

fn sameRepository(side: std.json.ObjectMap, repository: []const u8, require_non_fork: bool) bool {
    const repo = objectField(side, FIELD_REPO) orelse return false;
    if (!equalName(stringField(repo, FIELD_FULL_NAME), repository)) return false;
    if (!require_non_fork) return true;
    return switch (repo.get(FIELD_FORK) orelse return false) {
        .bool => |fork| !fork,
        else => false,
    };
}

fn isAppBot(login: []const u8, app_slug: []const u8) bool {
    return login.len == app_slug.len + BOT_SUFFIX.len and
        std.mem.startsWith(u8, login, app_slug) and
        std.mem.endsWith(u8, login, BOT_SUFFIX);
}

fn equalName(value: ?[]const u8, expected: []const u8) bool {
    return if (value) |actual| std.ascii.eqlIgnoreCase(actual, expected) else false;
}

fn objectField(object: std.json.ObjectMap, key: []const u8) ?std.json.ObjectMap {
    return switch (object.get(key) orelse return null) {
        .object => |value| value,
        else => null,
    };
}

fn stringField(object: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    return switch (object.get(key) orelse return null) {
        .string => |value| value,
        else => null,
    };
}

const TEST_APP_SLUG = "agentsfleet";
const TEST_BASE_BRANCH = "main";
const TEST_REPOSITORY = "o/r";
const TEST_VALID_BODY =
    \\{"repository":{"full_name":"o/r"},"pull_request":{"user":{"login":"agentsfleet[bot]"},"head":{"repo":{"full_name":"o/r","fork":false}},"base":{"ref":"main","repo":{"full_name":"o/r"}}}}
;

const RefusalCase = struct {
    body: []const u8,
    repository: []const u8 = TEST_REPOSITORY,
    base_branch: []const u8 = TEST_BASE_BRANCH,
    app_slug: []const u8 = TEST_APP_SLUG,
};

const REFUSAL_CASES = [_]RefusalCase{
    .{ .body = TEST_VALID_BODY, .app_slug = "" },
    .{ .body =
    \\{"repository":{"full_name":"other/r"},"pull_request":{"user":{"login":"agentsfleet[bot]"},"head":{"repo":{"full_name":"o/r","fork":false}},"base":{"ref":"main","repo":{"full_name":"o/r"}}}}
    },
    .{ .body =
    \\{"repository":{"full_name":"o/r"}}
    },
    .{ .body =
    \\{"repository":{"full_name":"o/r"},"pull_request":{"head":{"repo":{"full_name":"o/r","fork":false}},"base":{"ref":"main","repo":{"full_name":"o/r"}}}}
    },
    .{ .body =
    \\{"repository":{"full_name":"o/r"},"pull_request":{"user":{"login":"agentsfleet[bot]"},"base":{"ref":"main","repo":{"full_name":"o/r"}}}}
    },
    .{ .body =
    \\{"repository":{"full_name":"o/r"},"pull_request":{"user":{"login":"agentsfleet[bot]"},"head":{"repo":{"full_name":"o/r","fork":false}}}}
    },
    .{ .body =
    \\{"repository":{"full_name":"o/r"},"pull_request":{"user":{"login":"attacker"},"head":{"repo":{"full_name":"o/r","fork":false}},"base":{"ref":"main","repo":{"full_name":"o/r"}}}}
    },
    .{ .body =
    \\{"repository":{"full_name":"o/r"},"pull_request":{"user":{"login":"agentsfleet[bot]"},"head":{"repo":{"full_name":"other/r","fork":false}},"base":{"ref":"main","repo":{"full_name":"o/r"}}}}
    },
    .{ .body =
    \\{"repository":{"full_name":"o/r"},"pull_request":{"user":{"login":"agentsfleet[bot]"},"head":{"repo":{"full_name":"o/r","fork":false}},"base":{"ref":"main","repo":{"full_name":"other/r"}}}}
    },
    .{ .body =
    \\{"repository":{"full_name":"o/r"},"pull_request":{"user":{"login":"agentsfleet[bot]"},"head":{"repo":{"full_name":"o/r","fork":true}},"base":{"ref":"main","repo":{"full_name":"o/r"}}}}
    },
    .{ .body =
    \\{"repository":{"full_name":"o/r"},"pull_request":{"user":{"login":"agentsfleet[bot]"},"head":{"repo":{"full_name":"o/r"}},"base":{"ref":"main","repo":{"full_name":"o/r"}}}}
    },
    .{ .body =
    \\{"repository":{"full_name":"o/r"},"pull_request":{"user":{"login":"agentsfleet[bot]"},"head":{"repo":{"full_name":"o/r","fork":"false"}},"base":{"ref":"main","repo":{"full_name":"o/r"}}}}
    },
};

test "test_foreign_repair_pr_is_refused" {
    const body =
        \\{"installation":{"id":42},"repository":{"full_name":"o/r"},"pull_request":{"user":{"login":"attacker"},"head":{"repo":{"full_name":"o/r","fork":false}},"base":{"ref":"main","repo":{"full_name":"o/r"}}}}
    ;
    var parsed = try std.json.parseFromSlice(std.json.Value, std.testing.allocator, body, .{});
    defer parsed.deinit();
    try std.testing.expect(!ownedPullRequest(parsed.value.object, TEST_REPOSITORY, TEST_BASE_BRANCH, TEST_APP_SLUG));
}

test "test_own_repair_pr_links" {
    var parsed = try std.json.parseFromSlice(std.json.Value, std.testing.allocator, TEST_VALID_BODY, .{});
    defer parsed.deinit();
    try std.testing.expect(ownedPullRequest(parsed.value.object, TEST_REPOSITORY, TEST_BASE_BRANCH, TEST_APP_SLUG));
}

test "test_own_repair_pr_with_wrong_base_is_refused" {
    const body =
        \\{"installation":{"id":42},"repository":{"full_name":"o/r"},"pull_request":{"user":{"login":"agentsfleet[bot]"},"head":{"repo":{"full_name":"o/r","fork":false}},"base":{"ref":"develop","repo":{"full_name":"o/r"}}}}
    ;
    var parsed = try std.json.parseFromSlice(std.json.Value, std.testing.allocator, body, .{});
    defer parsed.deinit();
    try std.testing.expect(!ownedPullRequest(parsed.value.object, TEST_REPOSITORY, TEST_BASE_BRANCH, TEST_APP_SLUG));
}

test "repair Pull Request provenance rejects every incomplete or foreign boundary" {
    for (REFUSAL_CASES) |case| {
        var parsed = try std.json.parseFromSlice(std.json.Value, std.testing.allocator, case.body, .{});
        defer parsed.deinit();
        try std.testing.expect(!ownedPullRequest(
            parsed.value.object,
            case.repository,
            case.base_branch,
            case.app_slug,
        ));
    }
}
