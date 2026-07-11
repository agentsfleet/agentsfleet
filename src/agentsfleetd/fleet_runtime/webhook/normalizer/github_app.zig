//! GitHub App event classification and normalization over a caller-owned JSON
//! root. The ingress supplies the signed event header; this module never reads
//! headers, databases, or secrets.

const std = @import("std");
const workflow = @import("github.zig");

pub const EVENT_PULL_REQUEST = "pull_request";
pub const EVENT_WORKFLOW_RUN = "workflow_run";

const ACTION_COMPLETED = "completed";
const CONCLUSION_FAILURE = "failure";
const FIELD_ACTION = "action";
const FIELD_HEAD = "head";
const FIELD_NUMBER = "number";
const FIELD_REF = "ref";

pub const NormalizeError = error{
    MissingRepository,
    MissingPullRequest,
    MissingWorkflowRun,
    UnsupportedEvent,
};

/// Accepted carries caller-owned JSON; ignored carries a stable borrowed
/// reason. The caller frees only the accepted slice.
pub const Result = union(enum) {
    accepted: []u8,
    ignored: []const u8,
};

const PullRequest = struct {
    action: []const u8,
    repo: []const u8,
    number: i64,
    title: []const u8,
    url: []const u8,
    state: []const u8,
    draft: bool,
    author: []const u8,
    head_ref: []const u8,
    base_ref: []const u8,
    head_sha: []const u8,
    received_at: []const u8,
};

/// Extract the exact `owner/repository` routing identity from a signed payload.
pub fn repositoryFullName(root: std.json.ObjectMap) NormalizeError![]const u8 {
    const repo = objectField(root, "repository") orelse return NormalizeError.MissingRepository;
    return stringField(repo, "full_name") orelse return NormalizeError.MissingRepository;
}

/// Classify and normalize one supported GitHub App event.
pub fn normalizeFromValue(
    alloc: std.mem.Allocator,
    event: []const u8,
    root: std.json.ObjectMap,
    received_at_unix: i64,
) (std.mem.Allocator.Error || NormalizeError || workflow.NormalizeError)!Result {
    if (std.mem.eql(u8, event, EVENT_WORKFLOW_RUN)) {
        if (!std.mem.eql(u8, stringField(root, FIELD_ACTION) orelse "", ACTION_COMPLETED)) return .{ .ignored = "non_completed_action" };
        const run = objectField(root, EVENT_WORKFLOW_RUN) orelse return NormalizeError.MissingWorkflowRun;
        if (!std.mem.eql(u8, stringField(run, "conclusion") orelse "", CONCLUSION_FAILURE)) return .{ .ignored = "non_failure_conclusion" };
        return .{ .accepted = try workflow.normalizeFromValue(alloc, root, received_at_unix) };
    }
    if (!std.mem.eql(u8, event, EVENT_PULL_REQUEST)) return NormalizeError.UnsupportedEvent;
    return .{ .accepted = try normalizePullRequest(alloc, root, received_at_unix) };
}

/// Registry adapter: accepted events return owned JSON; deliberately ignored
/// workflow outcomes return null.
pub fn normalizeForIngress(
    alloc: std.mem.Allocator,
    event: []const u8,
    root: std.json.ObjectMap,
    received_at_unix: i64,
) anyerror!?[]u8 {
    const normalized = normalizeFromValue(alloc, event, root, received_at_unix) catch |err| {
        if (err == error.UnsupportedEvent) return null;
        return err;
    };
    return switch (normalized) {
        .accepted => |body| body,
        .ignored => null,
    };
}

fn normalizePullRequest(alloc: std.mem.Allocator, root: std.json.ObjectMap, received_at_unix: i64) ![]u8 {
    const pr = objectField(root, EVENT_PULL_REQUEST) orelse return NormalizeError.MissingPullRequest;
    var ts_buf: [32]u8 = undefined;
    return std.json.Stringify.valueAlloc(alloc, PullRequest{
        .action = stringField(root, FIELD_ACTION) orelse "",
        .repo = try repositoryFullName(root),
        .number = integerField(pr, FIELD_NUMBER) orelse integerField(root, FIELD_NUMBER) orelse 0,
        .title = stringField(pr, "title") orelse "",
        .url = stringField(pr, "html_url") orelse "",
        .state = stringField(pr, "state") orelse "",
        .draft = boolField(pr, "draft") orelse false,
        .author = nestedStringField(pr, "user", "login") orelse "",
        .head_ref = nestedStringField(pr, FIELD_HEAD, FIELD_REF) orelse "",
        .base_ref = nestedStringField(pr, "base", FIELD_REF) orelse "",
        .head_sha = nestedStringField(pr, FIELD_HEAD, "sha") orelse "",
        .received_at = workflow.formatRfc3339(&ts_buf, received_at_unix),
    }, .{});
}

fn objectField(obj: std.json.ObjectMap, key: []const u8) ?std.json.ObjectMap {
    const value = obj.get(key) orelse return null;
    return switch (value) {
        .object => |nested| nested,
        else => null,
    };
}

fn nestedStringField(obj: std.json.ObjectMap, outer: []const u8, inner: []const u8) ?[]const u8 {
    return stringField(objectField(obj, outer) orelse return null, inner);
}

fn stringField(obj: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    const value = obj.get(key) orelse return null;
    return switch (value) {
        .string => |s| s,
        else => null,
    };
}

fn integerField(obj: std.json.ObjectMap, key: []const u8) ?i64 {
    const value = obj.get(key) orelse return null;
    return switch (value) {
        .integer => |i| i,
        else => null,
    };
}

fn boolField(obj: std.json.ObjectMap, key: []const u8) ?bool {
    const value = obj.get(key) orelse return null;
    return if (value == .bool) value.bool else null;
}

test {
    _ = @import("github_app_test.zig");
}
