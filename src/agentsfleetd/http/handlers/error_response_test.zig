// Unit tests for errorResponse behavior.
//
// Uses httpz.testing to exercise the real response path — correct HTTP status,
// Content-Type: application/problem+json, and RFC 7807 JSON body shape.
//
// What is tested here (not in error_registry.zig):
//   Known code -> correct HTTP status in response
//   Content-Type header is application/problem+json
//   Body fields: docs_uri, title, detail, error_code, request_id all present
//   error_code uses caller-supplied code, not UNKNOWN.code
//   Unregistered code -> 500 with caller's code in body
//   detail and request_id pass through verbatim

const std = @import("std");
const httpz = @import("httpz");
const common = @import("common.zig");
const error_codes = @import("../../errors/error_registry.zig");
const LONG_DETAIL_LENGTH = 1000;

// Known code -> correct HTTP status.

test "UZ-AUTH-002 returns HTTP 401 Unauthorized" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    common.errorResponse(ht.res, error_codes.ERR_UNAUTHORIZED, "test detail", "req-001");
    try ht.expectStatus(401);
}

test "UZ-INTERNAL-001 returns HTTP 503 Service Unavailable" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    common.errorResponse(ht.res, error_codes.ERR_INTERNAL_DB_UNAVAILABLE, "db down", "req-002");
    try ht.expectStatus(503);
}

test "UZ-REQ-002 returns HTTP 413 Payload Too Large" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    common.errorResponse(ht.res, error_codes.ERR_PAYLOAD_TOO_LARGE, "too big", "req-003");
    try ht.expectStatus(413);
}

// Content-Type is application/problem+json.

test "Content-Type is application/problem+json" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    common.errorResponse(ht.res, error_codes.ERR_UNAUTHORIZED, "detail", "req-ct");
    try ht.expectHeader("Content-Type", "application/problem+json");
}

// Problem details body fields and verbatim pass-through.

test "body contains all required problem details fields" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    common.errorResponse(ht.res, error_codes.ERR_UNAUTHORIZED, "missing bearer token", "req-body-01");

    const json = try ht.getJson();
    const obj = json.object;

    try std.testing.expect(obj.get("docs_uri") != null);
    try std.testing.expect(obj.get("title") != null);
    try std.testing.expect(obj.get("detail") != null);
    try std.testing.expect(obj.get("error_code") != null);
    try std.testing.expect(obj.get("request_id") != null);
}

test "detail and request_id pass through verbatim" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    common.errorResponse(ht.res, error_codes.ERR_UNAUTHORIZED, "my custom detail", "xreq-999");

    const json = try ht.getJson();
    const obj = json.object;

    try std.testing.expectEqualStrings("my custom detail", obj.get("detail").?.string);
    try std.testing.expectEqualStrings("xreq-999", obj.get("request_id").?.string);
}

// error_code is the caller-supplied code, not UNKNOWN.code.

test "error_code in body is caller-supplied code, not UNKNOWN.code" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    // Pass an unregistered code — fallback to UNKNOWN for status/title,
    // but error_code must still be what the caller passed.
    const unregistered = "UZ-DOES-NOT-EXIST-IN-TABLE";
    common.errorResponse(ht.res, unregistered, "sentinel check", "req-sentinel");

    const json = try ht.getJson();
    const obj = json.object;

    try std.testing.expectEqualStrings(unregistered, obj.get("error_code").?.string);
    // Must NOT be the UNKNOWN sentinel code
    try std.testing.expect(!std.mem.eql(u8, error_codes.UNKNOWN.code, obj.get("error_code").?.string));
}

// Unregistered code -> 500, caller code preserved.

test "unregistered code returns HTTP 500 fallback" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    common.errorResponse(ht.res, "UZ-DOES-NOT-EXIST-IN-TABLE", "fallback test", "req-fb");
    try ht.expectStatus(500);
}

// Edge cases: empty and boundary inputs.

test "empty detail string passes through without crashing" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    common.errorResponse(ht.res, error_codes.ERR_UNAUTHORIZED, "", "req-t2a");
    try ht.expectStatus(401);
    const json = try ht.getJson();
    try std.testing.expectEqualStrings("", json.object.get("detail").?.string);
}

test "empty request_id string passes through without crashing" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    common.errorResponse(ht.res, error_codes.ERR_UNAUTHORIZED, "some detail", "");
    try ht.expectStatus(401);
    const json = try ht.getJson();
    try std.testing.expectEqualStrings("", json.object.get("request_id").?.string);
}

test "very long detail does not crash" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const long_detail = "x" ** 1000;
    common.errorResponse(ht.res, error_codes.ERR_UNAUTHORIZED, long_detail, "req-t2c");
    try ht.expectStatus(401);
    const json = try ht.getJson();
    try std.testing.expectEqual(@as(usize, LONG_DETAIL_LENGTH), json.object.get("detail").?.string.len);
}

test "docs_uri in body matches table entry and is not caller-constructed" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    common.errorResponse(ht.res, error_codes.ERR_UNAUTHORIZED, "d", "r");
    const json = try ht.getJson();
    const expected_entry = error_codes.lookup(error_codes.ERR_UNAUTHORIZED);
    try std.testing.expectEqualStrings(expected_entry.docs_uri, json.object.get("docs_uri").?.string);
}

test "title in body matches table entry and caller cannot override" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    common.errorResponse(ht.res, error_codes.ERR_UNAUTHORIZED, "d", "r");
    const json = try ht.getJson();
    const expected_entry = error_codes.lookup(error_codes.ERR_UNAUTHORIZED);
    try std.testing.expectEqualStrings(expected_entry.title, json.object.get("title").?.string);
}

// No nested error wrapper: problem details body is flat.

test "body has no nested error field" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    common.errorResponse(ht.res, error_codes.ERR_UNAUTHORIZED, "detail", "req-t3");
    const json = try ht.getJson();
    // Old format had .error.code — must not exist in new format
    try std.testing.expectEqual(@as(?std.json.Value, null), json.object.get("error"));
}

test "body has no nested message field" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    common.errorResponse(ht.res, error_codes.ERR_UNAUTHORIZED, "detail", "req-t3b");
    const json = try ht.getJson();
    // Old format used .message — replaced by .detail in RFC 7807
    try std.testing.expectEqual(@as(?std.json.Value, null), json.object.get("message"));
}

// Memory safety: repeated calls do not leak.

test "repeated errorResponse calls with std.testing.allocator do not leak" {
    // httpz.testing.init uses a fresh arena per call; we verify no arena leak
    // by ensuring each call pair (init/deinit) is balanced.
    var i: usize = 0;
    while (i < 100) : (i += 1) {
        var ht = httpz.testing.init(.{});
        defer ht.deinit();
        common.errorResponse(ht.res, error_codes.ERR_UNAUTHORIZED, "detail", "req-loop");
        try ht.expectStatus(401);
    }
}

// requireMethod — the single-method guard shared by the route_table invoke
// functions. Housed here (not inline in common.zig) to keep that file under the
// 350-line cap; uses the same httpz.testing harness.

test "requireMethod accepts a matching method and leaves the response untouched" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    try std.testing.expect(common.requireMethod(ht.res, .POST, .POST));
    // No side effect on a match: status stays at the default 200, body unset.
    try ht.expectStatus(200);
}

test "requireMethod rejects a mismatched method with 405" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    try std.testing.expect(!common.requireMethod(ht.res, .GET, .POST));
    try ht.expectStatus(405);
}
