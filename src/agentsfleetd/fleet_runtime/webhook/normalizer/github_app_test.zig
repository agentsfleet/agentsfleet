const std = @import("std");
const subject = @import("github_app.zig");

fn parse(src: []const u8) !std.json.Parsed(std.json.Value) {
    return std.json.parseFromSlice(std.json.Value, std.testing.allocator, src, .{});
}

test "GitHub App Pull Request normalizes repository and review context" {
    var payload = try parse(
        \\{"action":"opened","number":42,"repository":{"full_name":"agentsfleet/agentsfleet"},"pull_request":{"number":42,"title":"Fix routing","html_url":"https://github.com/agentsfleet/agentsfleet/pull/42","state":"open","draft":false,"user":{"login":"indy"},"head":{"ref":"fix","sha":"abc123"},"base":{"ref":"main"}}}
    );
    defer payload.deinit();
    const result = try subject.normalizeFromValue(std.testing.allocator, subject.EVENT_PULL_REQUEST, payload.value.object, 0);
    const body = result.accepted;
    defer std.testing.allocator.free(body);
    try std.testing.expect(std.mem.indexOf(u8, body, "\"repo\":\"agentsfleet/agentsfleet\"") != null);
    try std.testing.expect(std.mem.indexOf(u8, body, "\"number\":42") != null);
    try std.testing.expect(std.mem.indexOf(u8, body, "\"head_sha\":\"abc123\"") != null);
}

test "GitHub App workflow run accepts only completed failures" {
    var failed = try parse(
        \\{"action":"completed","repository":{"full_name":"agentsfleet/agentsfleet"},"workflow_run":{"id":7,"conclusion":"failure","html_url":"https://example.test/run/7"}}
    );
    defer failed.deinit();
    const accepted = try subject.normalizeFromValue(std.testing.allocator, subject.EVENT_WORKFLOW_RUN, failed.value.object, 0);
    defer std.testing.allocator.free(accepted.accepted);

    var success = try parse(
        \\{"action":"completed","repository":{"full_name":"agentsfleet/agentsfleet"},"workflow_run":{"id":8,"conclusion":"success"}}
    );
    defer success.deinit();
    const ignored = try subject.normalizeFromValue(std.testing.allocator, subject.EVENT_WORKFLOW_RUN, success.value.object, 0);
    try std.testing.expectEqualStrings("non_failure_conclusion", ignored.ignored);
}

test "GitHub App normalization rejects unsupported and malformed event shapes" {
    var empty = try parse("{}");
    defer empty.deinit();
    try std.testing.expectError(subject.NormalizeError.UnsupportedEvent, subject.normalizeFromValue(std.testing.allocator, "issues", empty.value.object, 0));
    try std.testing.expectError(subject.NormalizeError.MissingPullRequest, subject.normalizeFromValue(std.testing.allocator, subject.EVENT_PULL_REQUEST, empty.value.object, 0));

    var missing_repo = try parse(
        \\{"action":"opened","pull_request":{"number":1}}
    );
    defer missing_repo.deinit();
    try std.testing.expectError(subject.NormalizeError.MissingRepository, subject.normalizeFromValue(std.testing.allocator, subject.EVENT_PULL_REQUEST, missing_repo.value.object, 0));
}

test "GitHub App ingress normalizer ignores unsupported event" {
    var empty = try parse(
        \\{"installation":{"id":123},"repository":{"full_name":"agentsfleet/agentsfleet"}}
    );
    defer empty.deinit();

    const maybe = try subject.normalizeForIngress(std.testing.allocator, "issues", empty.value.object, 0);
    try std.testing.expectEqual(@as(?[]const u8, null), maybe);
}
