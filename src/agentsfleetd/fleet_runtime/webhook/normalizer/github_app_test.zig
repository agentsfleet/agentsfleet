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

test "GitHub App traffic on the crew's own repair branch is never an incident" {
    // A FAILED run on a repair branch is the one that matters: it is exactly
    // the shape that wakes a fleet, so without this guard the repairer ingests
    // its own broken repair as a fresh incident and sets off to fix what it
    // just wrote — a new approval card every cycle.
    var failed_repair = try parse(
        \\{"action":"completed","repository":{"full_name":"agentsfleet/agentsfleet"},"workflow_run":{"id":9,"conclusion":"failure","head_branch":"agentsfleet-repair/evt-1","html_url":"https://example.test/run/9"}}
    );
    defer failed_repair.deinit();
    const run_ignored = try subject.normalizeFromValue(std.testing.allocator, subject.EVENT_WORKFLOW_RUN, failed_repair.value.object, 0);
    try std.testing.expectEqualStrings("repair_branch", run_ignored.ignored);

    // The draft Pull Request the crew opened, echoing back.
    var repair_pr = try parse(
        \\{"action":"opened","number":43,"repository":{"full_name":"agentsfleet/agentsfleet"},"pull_request":{"number":43,"title":"repair","html_url":"https://example.test/pull/43","state":"open","draft":true,"user":{"login":"agentsfleet"},"head":{"ref":"agentsfleet-repair/evt-1","sha":"def456"},"base":{"ref":"main"}}}
    );
    defer repair_pr.deinit();
    const pr_ignored = try subject.normalizeFromValue(std.testing.allocator, subject.EVENT_PULL_REQUEST, repair_pr.value.object, 0);
    try std.testing.expectEqualStrings("repair_branch", pr_ignored.ignored);

    // An ordinary branch still normalizes — the guard is keyed to the prefix,
    // not to repair-shaped traffic in general.
    var ordinary = try parse(
        \\{"action":"completed","repository":{"full_name":"agentsfleet/agentsfleet"},"workflow_run":{"id":10,"conclusion":"failure","head_branch":"feat/ordinary","html_url":"https://example.test/run/10"}}
    );
    defer ordinary.deinit();
    const accepted = try subject.normalizeFromValue(std.testing.allocator, subject.EVENT_WORKFLOW_RUN, ordinary.value.object, 0);
    defer std.testing.allocator.free(accepted.accepted);
    try std.testing.expect(accepted == .accepted);
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

    try std.testing.expect((try subject.normalizeForIngress(std.testing.allocator, "issues", empty.value.object, 0)) == null);
}
