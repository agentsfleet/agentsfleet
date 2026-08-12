//! Pure normalization for GitHub `deployment_status` production evidence.

const std = @import("std");
const event_time = @import("../../../../state/fleet_events_filter.zig");

pub const EVENT_DEPLOYMENT_STATUS = "deployment_status";
pub const PRODUCTION_ENVIRONMENT = "production";
pub const STATE_SUCCESS = "success";
pub const STATE_FAILURE = "failure";
pub const STATE_ERROR = "error";
pub const STATE_INACTIVE = "inactive";

const FIELD_DEPLOYMENT_STATUS = "deployment_status";
const FIELD_DEPLOYMENT = "deployment";
const FIELD_REPOSITORY = "repository";
const FIELD_FULL_NAME = "full_name";
const FIELD_ID = "id";
const FIELD_ENVIRONMENT = "environment";
const FIELD_SHA = "sha";
const FIELD_STATE = "state";
const FIELD_UPDATED_AT = "updated_at";
const IGNORE_INVALID_COMPLETION_TIME = "invalid_completion_time";
const IGNORE_MISSING_COMMIT_SHA = "missing_commit_sha";
const IGNORE_MISSING_COMPLETION_TIME = "missing_completion_time";
const IGNORE_MISSING_DEPLOYMENT = "missing_deployment";
const IGNORE_MISSING_DEPLOYMENT_ID = "missing_deployment_id";
const IGNORE_MISSING_DEPLOYMENT_STATE = "missing_deployment_state";
const IGNORE_MISSING_DEPLOYMENT_STATUS = "missing_deployment_status";
const IGNORE_MISSING_DEPLOYMENT_STATUS_ID = "missing_deployment_status_id";
const IGNORE_MISSING_ENVIRONMENT = "missing_environment";
const IGNORE_MISSING_REPOSITORY = "missing_repository";
const IGNORE_MISSING_REPOSITORY_NAME = "missing_repository_name";
const IGNORE_NON_PRODUCTION_ENVIRONMENT = "non_production_environment";
const IGNORE_NON_TERMINAL_DEPLOYMENT_STATE = "non_terminal_deployment_state";

pub const Production = struct {
    provider_deployment_id: i64,
    provider_status_id: i64,
    repository: []const u8,
    environment: []const u8,
    commit_sha: []const u8,
    conclusion: []const u8,
    completed_at: i64,
};

/// Accepted data borrows the caller-owned parsed payload. Ignored reasons are
/// stable constants for logs and tests; neither variant allocates.
pub const Result = union(enum) {
    production: Production,
    ignored: []const u8,
};

pub fn normalize(root: std.json.ObjectMap) Result {
    const status = objectField(root, FIELD_DEPLOYMENT_STATUS) orelse return .{ .ignored = IGNORE_MISSING_DEPLOYMENT_STATUS };
    const deployment = objectField(root, FIELD_DEPLOYMENT) orelse return .{ .ignored = IGNORE_MISSING_DEPLOYMENT };
    const repository = objectField(root, FIELD_REPOSITORY) orelse return .{ .ignored = IGNORE_MISSING_REPOSITORY };
    const environment = nonEmpty(stringField(status, FIELD_ENVIRONMENT) orelse stringField(deployment, FIELD_ENVIRONMENT)) orelse return .{ .ignored = IGNORE_MISSING_ENVIRONMENT };
    if (!std.ascii.eqlIgnoreCase(environment, PRODUCTION_ENVIRONMENT)) return .{ .ignored = IGNORE_NON_PRODUCTION_ENVIRONMENT };
    const conclusion = nonEmpty(stringField(status, FIELD_STATE)) orelse return .{ .ignored = IGNORE_MISSING_DEPLOYMENT_STATE };
    if (!isTerminal(conclusion)) return .{ .ignored = IGNORE_NON_TERMINAL_DEPLOYMENT_STATE };
    const completed_at = timestamp(nonEmpty(stringField(status, FIELD_UPDATED_AT)) orelse return .{ .ignored = IGNORE_MISSING_COMPLETION_TIME }) orelse return .{ .ignored = IGNORE_INVALID_COMPLETION_TIME };
    return .{ .production = .{
        .provider_deployment_id = intField(deployment, FIELD_ID) orelse return .{ .ignored = IGNORE_MISSING_DEPLOYMENT_ID },
        .provider_status_id = intField(status, FIELD_ID) orelse return .{ .ignored = IGNORE_MISSING_DEPLOYMENT_STATUS_ID },
        .repository = nonEmpty(stringField(repository, FIELD_FULL_NAME)) orelse return .{ .ignored = IGNORE_MISSING_REPOSITORY_NAME },
        .environment = environment,
        .commit_sha = nonEmpty(stringField(deployment, FIELD_SHA)) orelse return .{ .ignored = IGNORE_MISSING_COMMIT_SHA },
        .conclusion = conclusion,
        .completed_at = completed_at,
    } };
}

fn isTerminal(value: []const u8) bool {
    return std.mem.eql(u8, value, STATE_SUCCESS) or
        std.mem.eql(u8, value, STATE_FAILURE) or
        std.mem.eql(u8, value, STATE_ERROR) or
        std.mem.eql(u8, value, STATE_INACTIVE);
}

fn timestamp(value: []const u8) ?i64 {
    if (value.len != 20 or value[value.len - 1] != 'Z') return null;
    return event_time.parseSince(value, 0) catch null;
}

fn objectField(object: std.json.ObjectMap, key: []const u8) ?std.json.ObjectMap {
    return switch (object.get(key) orelse return null) {
        .object => |nested| nested,
        else => null,
    };
}

fn stringField(object: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    return switch (object.get(key) orelse return null) {
        .string => |value| value,
        else => null,
    };
}

fn intField(object: std.json.ObjectMap, key: []const u8) ?i64 {
    return switch (object.get(key) orelse return null) {
        .integer => |value| value,
        else => null,
    };
}

fn nonEmpty(value: ?[]const u8) ?[]const u8 {
    const text = value orelse return null;
    return if (text.len == 0) null else text;
}

test "test_github_production_status_normalizes" {
    const body =
        \\{"repository":{"full_name":"agentsfleet/agentsfleet"},"deployment":{"id":7,"sha":"abc123"},"deployment_status":{"id":42,"environment":"production","state":"success","updated_at":"2026-08-10T12:00:00Z"}}
    ;
    var parsed = try std.json.parseFromSlice(std.json.Value, std.testing.allocator, body, .{});
    defer parsed.deinit();
    const result = normalize(parsed.value.object);
    const production = switch (result) {
        .production => |value| value,
        .ignored => return error.TestUnexpectedResult,
    };
    try std.testing.expectEqual(@as(i64, 7), production.provider_deployment_id);
    try std.testing.expectEqual(@as(i64, 42), production.provider_status_id);
    try std.testing.expectEqualStrings("agentsfleet/agentsfleet", production.repository);
    try std.testing.expectEqualStrings("abc123", production.commit_sha);
}

test "test_vercel_github_status_normalizes" {
    const body =
        \\{"repository":{"full_name":"agentsfleet/agentsfleet"},"deployment":{"id":8,"sha":"vercel-commit","environment":"production"},"deployment_status":{"id":43,"state":"success","updated_at":"2026-08-10T12:00:00Z"}}
    ;
    var parsed = try std.json.parseFromSlice(std.json.Value, std.testing.allocator, body, .{});
    defer parsed.deinit();
    try std.testing.expect(switch (normalize(parsed.value.object)) {
        .production => true,
        .ignored => false,
    });
}

test "test_unready_deployment_status_is_ignored" {
    const body =
        \\{"repository":{"full_name":"agentsfleet/agentsfleet"},"deployment":{"id":7,"sha":"abc123"},"deployment_status":{"id":42,"environment":"preview","state":"pending","updated_at":"2026-08-10T12:00:00Z"}}
    ;
    var parsed = try std.json.parseFromSlice(std.json.Value, std.testing.allocator, body, .{});
    defer parsed.deinit();
    try std.testing.expect(switch (normalize(parsed.value.object)) {
        .production => false,
        .ignored => true,
    });
}

test "test_incomplete_deployment_identity_is_ignored" {
    const cases = [_]struct {
        body: []const u8,
        reason: []const u8,
    }{
        .{ .body = "{}", .reason = IGNORE_MISSING_DEPLOYMENT_STATUS },
        .{
            .body =
            \\{"deployment_status":{"id":42,"environment":"production","state":"success","updated_at":"2026-08-10T12:00:00Z"}}
            ,
            .reason = IGNORE_MISSING_DEPLOYMENT,
        },
        .{
            .body =
            \\{"deployment":{"id":7,"sha":"abc123"},"deployment_status":{"id":42,"environment":"production","state":"success","updated_at":"2026-08-10T12:00:00Z"}}
            ,
            .reason = IGNORE_MISSING_REPOSITORY,
        },
        .{
            .body =
            \\{"repository":{"full_name":"agentsfleet/agentsfleet"},"deployment":{"id":7,"sha":"abc123"},"deployment_status":{"id":42,"state":"success","updated_at":"2026-08-10T12:00:00Z"}}
            ,
            .reason = IGNORE_MISSING_ENVIRONMENT,
        },
        .{
            .body =
            \\{"repository":{"full_name":"agentsfleet/agentsfleet"},"deployment":{"id":7,"sha":"abc123"},"deployment_status":{"id":42,"environment":"preview","state":"success","updated_at":"2026-08-10T12:00:00Z"}}
            ,
            .reason = IGNORE_NON_PRODUCTION_ENVIRONMENT,
        },
        .{
            .body =
            \\{"repository":{"full_name":"agentsfleet/agentsfleet"},"deployment":{"id":7,"sha":"abc123"},"deployment_status":{"id":42,"environment":"production","updated_at":"2026-08-10T12:00:00Z"}}
            ,
            .reason = IGNORE_MISSING_DEPLOYMENT_STATE,
        },
        .{
            .body =
            \\{"repository":{"full_name":"agentsfleet/agentsfleet"},"deployment":{"id":7,"sha":"abc123"},"deployment_status":{"id":42,"environment":"production","state":"pending","updated_at":"2026-08-10T12:00:00Z"}}
            ,
            .reason = IGNORE_NON_TERMINAL_DEPLOYMENT_STATE,
        },
        .{
            .body =
            \\{"repository":{"full_name":"agentsfleet/agentsfleet"},"deployment":{"id":7,"sha":"abc123"},"deployment_status":{"id":42,"environment":"production","state":"success"}}
            ,
            .reason = IGNORE_MISSING_COMPLETION_TIME,
        },
        .{
            .body =
            \\{"repository":{"full_name":"agentsfleet/agentsfleet"},"deployment":{"id":7,"sha":"abc123"},"deployment_status":{"id":42,"environment":"production","state":"success","updated_at":"invalid"}}
            ,
            .reason = IGNORE_INVALID_COMPLETION_TIME,
        },
        .{
            .body =
            \\{"repository":{"full_name":"agentsfleet/agentsfleet"},"deployment":{"sha":"abc123"},"deployment_status":{"id":42,"environment":"production","state":"success","updated_at":"2026-08-10T12:00:00Z"}}
            ,
            .reason = IGNORE_MISSING_DEPLOYMENT_ID,
        },
        .{
            .body =
            \\{"repository":{"full_name":"agentsfleet/agentsfleet"},"deployment":{"id":7,"sha":"abc123"},"deployment_status":{"environment":"production","state":"success","updated_at":"2026-08-10T12:00:00Z"}}
            ,
            .reason = IGNORE_MISSING_DEPLOYMENT_STATUS_ID,
        },
        .{
            .body =
            \\{"repository":{},"deployment":{"id":7,"sha":"abc123"},"deployment_status":{"id":42,"environment":"production","state":"success","updated_at":"2026-08-10T12:00:00Z"}}
            ,
            .reason = IGNORE_MISSING_REPOSITORY_NAME,
        },
        .{
            .body =
            \\{"repository":{"full_name":"agentsfleet/agentsfleet"},"deployment":{"id":7,"sha":""},"deployment_status":{"id":42,"environment":"production","state":"success","updated_at":"2026-08-10T12:00:00Z"}}
            ,
            .reason = IGNORE_MISSING_COMMIT_SHA,
        },
    };
    for (cases) |case| {
        var parsed = try std.json.parseFromSlice(std.json.Value, std.testing.allocator, case.body, .{});
        defer parsed.deinit();
        const reason = switch (normalize(parsed.value.object)) {
            .production => return error.TestUnexpectedResult,
            .ignored => |value| value,
        };
        try std.testing.expectEqualStrings(case.reason, reason);
    }
}

test "every terminal deployment state is accepted" {
    inline for (.{ STATE_SUCCESS, STATE_FAILURE, STATE_ERROR, STATE_INACTIVE }) |state| {
        try std.testing.expect(isTerminal(state));
    }
}
