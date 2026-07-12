// GitHub webhook ingest filter — decides whether a parsed event payload
// should be added to the fleet's event stream. Pure functions
// over `std.json.Value`; no I/O, no logging, no allocations beyond what
// the caller already owns.
//
// Failed completed workflow runs are accepted. Pull requests are accepted
// when they open, reopen, receive new commits, or leave draft state.

const std = @import("std");

const S_WORKFLOW_RUN = "workflow_run";
const S_PULL_REQUEST = "pull_request";

pub const FilterDecision = struct {
    ingest: bool,
    reason: []const u8,
};

const ACTION_COMPLETED = "completed";
const CONCLUSION_FAILURE = "failure";
const FIELD_ACTION = "action";
const FIELD_REPOSITORY = "repository";
const REASON_MISSING_REPOSITORY = "missing_repository";
pub const EVENT_WORKFLOW_RUN = S_WORKFLOW_RUN;
pub const EVENT_PULL_REQUEST = S_PULL_REQUEST;

pub fn isSupportedEvent(event: []const u8) bool {
    return std.mem.eql(u8, event, EVENT_WORKFLOW_RUN) or std.mem.eql(u8, event, EVENT_PULL_REQUEST);
}

pub fn filterParsedRoot(event: []const u8, root: std.json.ObjectMap) ?FilterDecision {
    if (std.mem.eql(u8, event, EVENT_PULL_REQUEST)) return filterPullRequest(root);
    if (!std.mem.eql(u8, event, EVENT_WORKFLOW_RUN)) return null;
    return filterWorkflowRun(root);
}

fn filterWorkflowRun(root: std.json.ObjectMap) ?FilterDecision {
    const action = stringField(root.get(FIELD_ACTION)) orelse "";
    if (!std.mem.eql(u8, action, ACTION_COMPLETED)) {
        return .{ .ingest = false, .reason = "non_completed_action" };
    }
    const wr = switch (root.get(S_WORKFLOW_RUN) orelse return null) {
        .object => |o| o,
        else => return null,
    };
    const conclusion = stringField(wr.get("conclusion")) orelse "";
    if (!std.mem.eql(u8, conclusion, CONCLUSION_FAILURE)) {
        return .{ .ingest = false, .reason = "non_failure_conclusion" };
    }
    const repo_ok = if (root.get(FIELD_REPOSITORY)) |v| v == .object else false;
    if (!repo_ok) return .{ .ingest = false, .reason = REASON_MISSING_REPOSITORY };
    return .{ .ingest = true, .reason = "" };
}

fn filterPullRequest(root: std.json.ObjectMap) ?FilterDecision {
    const action = stringField(root.get(FIELD_ACTION)) orelse "";
    const accepted = std.mem.eql(u8, action, "opened") or
        std.mem.eql(u8, action, "reopened") or
        std.mem.eql(u8, action, "synchronize") or
        std.mem.eql(u8, action, "ready_for_review");
    if (!accepted) return .{ .ingest = false, .reason = "non_review_action" };
    const pull_request = root.get(S_PULL_REQUEST) orelse return null;
    if (pull_request != .object) return null;
    const repo_ok = if (root.get(FIELD_REPOSITORY)) |value| value == .object else false;
    if (!repo_ok) return .{ .ingest = false, .reason = REASON_MISSING_REPOSITORY };
    return .{ .ingest = true, .reason = "" };
}

fn filterAction(alloc: std.mem.Allocator, event: []const u8, body: []const u8) ?FilterDecision {
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, body, .{}) catch return null;
    defer parsed.deinit();
    return switch (parsed.value) {
        .object => |o| filterParsedRoot(event, o),
        else => null,
    };
}

fn stringField(v: ?std.json.Value) ?[]const u8 {
    const val = v orelse return null;
    return switch (val) {
        .string => |s| s,
        else => null,
    };
}

const testing = std.testing;

test "filterAction: completed + failure + repository → ingest" {
    const body =
        \\{"action":"completed","workflow_run":{"conclusion":"failure"},"repository":{"full_name":"o/r"}}
    ;
    const got = filterAction(testing.allocator, EVENT_WORKFLOW_RUN, body) orelse return error.TestUnexpectedNull;
    try testing.expect(got.ingest);
}

test "filterAction: completed + failure but missing repository → ignore missing_repository" {
    const body =
        \\{"action":"completed","workflow_run":{"conclusion":"failure"}}
    ;
    const got = filterAction(testing.allocator, EVENT_WORKFLOW_RUN, body) orelse return error.TestUnexpectedNull;
    try testing.expect(!got.ingest);
    try testing.expectEqualStrings(REASON_MISSING_REPOSITORY, got.reason);
}

test "filterAction: in_progress action → ignore non_completed_action" {
    const body =
        \\{"action":"in_progress","workflow_run":{"conclusion":null}}
    ;
    const got = filterAction(testing.allocator, EVENT_WORKFLOW_RUN, body) orelse return error.TestUnexpectedNull;
    try testing.expect(!got.ingest);
    try testing.expectEqualStrings("non_completed_action", got.reason);
}

test "filterAction: missing action → ignore non_completed_action" {
    const body =
        \\{"workflow_run":{"conclusion":"failure"}}
    ;
    const got = filterAction(testing.allocator, EVENT_WORKFLOW_RUN, body) orelse return error.TestUnexpectedNull;
    try testing.expect(!got.ingest);
    try testing.expectEqualStrings("non_completed_action", got.reason);
}

test "filterAction: missing workflow_run → null" {
    const body =
        \\{"action":"completed"}
    ;
    try testing.expect(filterAction(testing.allocator, EVENT_WORKFLOW_RUN, body) == null);
}

test "filterAction: malformed JSON → null" {
    try testing.expect(filterAction(testing.allocator, EVENT_WORKFLOW_RUN, "not json") == null);
}

test "filterAction: non-object root → null" {
    try testing.expect(filterAction(testing.allocator, EVENT_WORKFLOW_RUN, "[1,2,3]") == null);
}

test "filterAction: parameterized non-failure conclusions" {
    const cases = [_][]const u8{
        \\{"action":"completed","workflow_run":{"conclusion":"success"}}
        ,
        \\{"action":"completed","workflow_run":{"conclusion":"neutral"}}
        ,
        \\{"action":"completed","workflow_run":{"conclusion":"skipped"}}
        ,
        \\{"action":"completed","workflow_run":{"conclusion":"timed_out"}}
        ,
        \\{"action":"completed","workflow_run":{"conclusion":"action_required"}}
        ,
    };
    for (cases) |body| {
        const got = filterAction(testing.allocator, EVENT_WORKFLOW_RUN, body) orelse return error.TestUnexpectedNull;
        try testing.expect(!got.ingest);
        try testing.expectEqualStrings("non_failure_conclusion", got.reason);
    }
}

test "filter constants pin" {
    try testing.expectEqualStrings("workflow_run", EVENT_WORKFLOW_RUN);
    try testing.expectEqualStrings("pull_request", EVENT_PULL_REQUEST);
    try testing.expectEqualStrings("completed", ACTION_COMPLETED);
    try testing.expectEqualStrings("failure", CONCLUSION_FAILURE);
}

test "filterAction: opened pull request with repository → ingest" {
    const body =
        \\{"action":"opened","pull_request":{"number":42},"repository":{"full_name":"o/r"}}
    ;
    const got = filterAction(testing.allocator, EVENT_PULL_REQUEST, body) orelse return error.TestUnexpectedNull;
    try testing.expect(got.ingest);
}

test "filterAction: closed pull request → ignore non_review_action" {
    const body =
        \\{"action":"closed","pull_request":{"number":42},"repository":{"full_name":"o/r"}}
    ;
    const got = filterAction(testing.allocator, EVENT_PULL_REQUEST, body) orelse return error.TestUnexpectedNull;
    try testing.expect(!got.ingest);
    try testing.expectEqualStrings("non_review_action", got.reason);
}
