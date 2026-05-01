//! Unit tests for the GitHub workflow_run webhook normalizer.

const std = @import("std");
const testing = std.testing;
const github = @import("github.zig");

const FAILURE_FIXTURE = @embedFile("workflow_run_failure.json");
const SUCCESS_FIXTURE = @embedFile("workflow_run_success.json");

// Fixed RFC3339 timestamp for deterministic tests: 1970-01-01T00:00:00Z (epoch).
const FIXED_RECEIVED_AT_UNIX: i64 = 0;
const FIXED_RECEIVED_AT_RFC3339 = "1970-01-01T00:00:00Z";

fn parseObject(alloc: std.mem.Allocator, json: []const u8) !std.json.Parsed(std.json.Value) {
    return std.json.parseFromSlice(std.json.Value, alloc, json, .{});
}

fn fieldString(obj: std.json.ObjectMap, key: []const u8) []const u8 {
    const v = obj.get(key) orelse return "";
    return switch (v) {
        .string => |s| s,
        else => "",
    };
}

fn fieldI64(obj: std.json.ObjectMap, key: []const u8) i64 {
    const v = obj.get(key) orelse return 0;
    return switch (v) {
        .integer => |i| i,
        .float => |f| @intFromFloat(f),
        else => 0,
    };
}

test "normalize: failure payload — flat envelope shape with all fields" {
    const alloc = testing.allocator;

    const out = try github.normalize(alloc, FAILURE_FIXTURE, FIXED_RECEIVED_AT_UNIX);
    defer alloc.free(out);

    var parsed = try parseObject(alloc, out);
    defer parsed.deinit();

    const obj = parsed.value.object;
    try testing.expectEqualStrings("https://github.com/example/platform/actions/runs/9842731401", fieldString(obj, "run_url"));
    try testing.expectEqualStrings("f1d2c3b4a5968778695a4b3c2d1e0f9a8b7c6d5e", fieldString(obj, "head_sha"));
    try testing.expectEqualStrings("failure", fieldString(obj, "conclusion"));
    try testing.expectEqualStrings("main", fieldString(obj, "ref"));
    try testing.expectEqualStrings("example/platform", fieldString(obj, "repo"));
    try testing.expectEqual(@as(i64, 2), fieldI64(obj, "attempt"));
    try testing.expectEqual(@as(i64, 9842731401), fieldI64(obj, "run_id"));
    try testing.expectEqualStrings("main", fieldString(obj, "head_branch"));
    try testing.expectEqualStrings("deploy", fieldString(obj, "workflow_name"));
    try testing.expectEqualStrings(FIXED_RECEIVED_AT_RFC3339, fieldString(obj, "received_at"));
}

test "normalize: success payload — conclusion field is success" {
    const alloc = testing.allocator;

    const out = try github.normalize(alloc, SUCCESS_FIXTURE, FIXED_RECEIVED_AT_UNIX);
    defer alloc.free(out);

    var parsed = try parseObject(alloc, out);
    defer parsed.deinit();

    const obj = parsed.value.object;
    try testing.expectEqualStrings("success", fieldString(obj, "conclusion"));
    try testing.expectEqual(@as(i64, 1), fieldI64(obj, "attempt"));
}

test "normalize: result is a flat object (no nested message/metadata)" {
    const alloc = testing.allocator;

    const out = try github.normalize(alloc, FAILURE_FIXTURE, FIXED_RECEIVED_AT_UNIX);
    defer alloc.free(out);

    // Asserts the spec's A4 contract: the request_json is FLAT, not nested
    // under {message, metadata} like older drafts proposed.
    try testing.expect(std.mem.indexOf(u8, out, "\"message\":") == null);
    try testing.expect(std.mem.indexOf(u8, out, "\"metadata\":") == null);
    try testing.expect(std.mem.indexOf(u8, out, "\"run_id\":") != null);
    try testing.expect(std.mem.indexOf(u8, out, "\"head_sha\":") != null);
}

test "normalize: malformed JSON returns MalformedJson" {
    const alloc = testing.allocator;
    const got = github.normalize(alloc, "not a json", 0);
    try testing.expectError(github.NormalizeError.MalformedJson, got);
}

test "normalize: missing workflow_run returns MissingWorkflowRun" {
    const alloc = testing.allocator;
    const body =
        \\{"action":"completed","repository":{"full_name":"x/y"}}
    ;
    const got = github.normalize(alloc, body, 0);
    try testing.expectError(github.NormalizeError.MissingWorkflowRun, got);
}

test "normalize: missing repository returns MissingRepository" {
    const alloc = testing.allocator;
    const body =
        \\{"action":"completed","workflow_run":{"id":1}}
    ;
    const got = github.normalize(alloc, body, 0);
    try testing.expectError(github.NormalizeError.MissingRepository, got);
}

test "normalize: defaults attempt to 1 when run_attempt missing" {
    const alloc = testing.allocator;
    const body =
        \\{"workflow_run":{"id":42,"head_sha":"abc","conclusion":"failure","head_branch":"main","html_url":"u","name":"w"},"repository":{"full_name":"o/r"}}
    ;
    const out = try github.normalize(alloc, body, 0);
    defer alloc.free(out);

    var parsed = try parseObject(alloc, out);
    defer parsed.deinit();
    try testing.expectEqual(@as(i64, 1), fieldI64(parsed.value.object, "attempt"));
}

test "normalize: numeric run_id accepted as i64 even when serialized as float" {
    const alloc = testing.allocator;
    // GH historically emits IDs as integers but a defensive parser must
    // accept floats too — std.json may classify large numbers either way.
    const body =
        \\{"workflow_run":{"id":1.0e10,"run_attempt":1,"head_sha":"a","conclusion":"failure","head_branch":"m","html_url":"u","name":"w"},"repository":{"full_name":"o/r"}}
    ;
    const out = try github.normalize(alloc, body, 0);
    defer alloc.free(out);

    var parsed = try parseObject(alloc, out);
    defer parsed.deinit();
    try testing.expectEqual(@as(i64, 10000000000), fieldI64(parsed.value.object, "run_id"));
}

test "normalize: empty repository.full_name produces empty repo field" {
    const alloc = testing.allocator;
    const body =
        \\{"workflow_run":{"id":1,"run_attempt":1,"conclusion":"failure"},"repository":{"full_name":""}}
    ;
    const out = try github.normalize(alloc, body, 0);
    defer alloc.free(out);

    var parsed = try parseObject(alloc, out);
    defer parsed.deinit();
    try testing.expectEqualStrings("", fieldString(parsed.value.object, "repo"));
}

test "normalize: unicode workflow_name round-trips through JSON" {
    const alloc = testing.allocator;
    const body =
        \\{"workflow_run":{"id":1,"run_attempt":1,"conclusion":"failure","name":"deploy 🚀 — 部署"},"repository":{"full_name":"o/r"}}
    ;
    const out = try github.normalize(alloc, body, 0);
    defer alloc.free(out);

    var parsed = try parseObject(alloc, out);
    defer parsed.deinit();
    try testing.expectEqualStrings("deploy 🚀 — 部署", fieldString(parsed.value.object, "workflow_name"));
}

test "normalize: negative received_at clamps to epoch" {
    const alloc = testing.allocator;
    const out = try github.normalize(alloc, FAILURE_FIXTURE, -1);
    defer alloc.free(out);

    var parsed = try parseObject(alloc, out);
    defer parsed.deinit();
    try testing.expectEqualStrings("1970-01-01T00:00:00Z", fieldString(parsed.value.object, "received_at"));
}
