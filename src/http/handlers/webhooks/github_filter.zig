// GitHub webhook ingest filter — decides whether a parsed `workflow_run`
// payload should be XADDed to the zombie's event stream. Pure functions
// over `std.json.Value`; no I/O, no logging, no allocations beyond what
// the caller already owns.
//
// Filter contract: only `workflow_run` events with `action=completed` and
// `conclusion=failure` and a `repository` object are ingested. Everything
// else returns a `FilterDecision` with `ingest=false` and a stable
// machine-readable `reason` string for the caller to surface.

const std = @import("std");

pub const FilterDecision = struct {
    ingest: bool,
    reason: []const u8,
};

const ACTION_COMPLETED = "completed";
const CONCLUSION_FAILURE = "failure";
pub const EVENT_WORKFLOW_RUN = "workflow_run";

pub fn filterParsedRoot(root: std.json.ObjectMap) ?FilterDecision {
    const action = stringField(root.get("action")) orelse "";
    if (!std.mem.eql(u8, action, ACTION_COMPLETED)) {
        return .{ .ingest = false, .reason = "non_completed_action" };
    }
    const wr = switch (root.get("workflow_run") orelse return null) {
        .object => |o| o,
        else => return null,
    };
    const conclusion = stringField(wr.get("conclusion")) orelse "";
    if (!std.mem.eql(u8, conclusion, CONCLUSION_FAILURE)) {
        return .{ .ingest = false, .reason = "non_failure_conclusion" };
    }
    const repo_ok = if (root.get("repository")) |v| v == .object else false;
    if (!repo_ok) return .{ .ingest = false, .reason = "missing_repository" };
    return .{ .ingest = true, .reason = "" };
}

fn filterAction(alloc: std.mem.Allocator, body: []const u8) ?FilterDecision {
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, body, .{}) catch return null;
    defer parsed.deinit();
    return switch (parsed.value) {
        .object => |o| filterParsedRoot(o),
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
    const got = filterAction(testing.allocator, body) orelse return error.TestUnexpectedNull;
    try testing.expect(got.ingest);
}

test "filterAction: completed + failure but missing repository → ignore missing_repository" {
    const body =
        \\{"action":"completed","workflow_run":{"conclusion":"failure"}}
    ;
    const got = filterAction(testing.allocator, body) orelse return error.TestUnexpectedNull;
    try testing.expect(!got.ingest);
    try testing.expectEqualStrings("missing_repository", got.reason);
}

test "filterAction: in_progress action → ignore non_completed_action" {
    const body =
        \\{"action":"in_progress","workflow_run":{"conclusion":null}}
    ;
    const got = filterAction(testing.allocator, body) orelse return error.TestUnexpectedNull;
    try testing.expect(!got.ingest);
    try testing.expectEqualStrings("non_completed_action", got.reason);
}

test "filterAction: missing action → ignore non_completed_action" {
    const body =
        \\{"workflow_run":{"conclusion":"failure"}}
    ;
    const got = filterAction(testing.allocator, body) orelse return error.TestUnexpectedNull;
    try testing.expect(!got.ingest);
    try testing.expectEqualStrings("non_completed_action", got.reason);
}

test "filterAction: missing workflow_run → null" {
    const body =
        \\{"action":"completed"}
    ;
    try testing.expect(filterAction(testing.allocator, body) == null);
}

test "filterAction: malformed JSON → null" {
    try testing.expect(filterAction(testing.allocator, "not json") == null);
}

test "filterAction: non-object root → null" {
    try testing.expect(filterAction(testing.allocator, "[1,2,3]") == null);
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
        const got = filterAction(testing.allocator, body) orelse return error.TestUnexpectedNull;
        try testing.expect(!got.ingest);
        try testing.expectEqualStrings("non_failure_conclusion", got.reason);
    }
}

test "filter constants pin" {
    try testing.expectEqualStrings("workflow_run", EVENT_WORKFLOW_RUN);
    try testing.expectEqualStrings("completed", ACTION_COMPLETED);
    try testing.expectEqualStrings("failure", CONCLUSION_FAILURE);
}
