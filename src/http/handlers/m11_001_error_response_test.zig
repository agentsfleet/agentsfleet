// Unit tests for M11_001 errorResponse contract.
//
// Uses httpz.testing to exercise the real response path — correct HTTP status,
// Content-Type: application/problem+json, and RFC 7807 JSON body shape.
//
// What is tested here (not in error_table.zig):
//   T1  — Known code → correct HTTP status in response
//   T2  — Content-Type header is application/problem+json
//   T3  — Body fields: docs_uri, title, detail, error_code, request_id all present
//   T4  — error_code uses caller-supplied code, not UNKNOWN_ENTRY.code
//   T5  — Unregistered code → 500 with caller's code in body
//   T6  — detail and request_id pass through verbatim

const std = @import("std");
const httpz = @import("httpz");
const common = @import("common.zig");
const error_codes = @import("../../errors/codes.zig");
const error_table = @import("../../errors/error_table.zig");

// ── T1: Known code → correct HTTP status ─────────────────────────────────────

test "M11_001: UZ-AUTH-002 → HTTP 401 Unauthorized" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    common.errorResponse(ht.res, error_codes.ERR_UNAUTHORIZED, "test detail", "req-001");
    try ht.expectStatus(401);
}

test "M11_001: UZ-INTERNAL-001 → HTTP 503 Service Unavailable" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    common.errorResponse(ht.res, error_codes.ERR_INTERNAL_DB_UNAVAILABLE, "db down", "req-002");
    try ht.expectStatus(503);
}

test "M11_001: UZ-REQ-002 → HTTP 413 Payload Too Large" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    common.errorResponse(ht.res, error_codes.ERR_PAYLOAD_TOO_LARGE, "too big", "req-003");
    try ht.expectStatus(413);
}

// ── T2: Content-Type is application/problem+json ─────────────────────────────

test "M11_001: Content-Type is application/problem+json" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    common.errorResponse(ht.res, error_codes.ERR_UNAUTHORIZED, "detail", "req-ct");
    try ht.expectHeader("Content-Type", "application/problem+json");
}

// ── T3 + T6: RFC 7807 body fields present and verbatim pass-through ──────────

test "M11_001: body contains all required RFC 7807 fields" {
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

test "M11_001: detail and request_id pass through verbatim" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    common.errorResponse(ht.res, error_codes.ERR_UNAUTHORIZED, "my custom detail", "xreq-999");

    const json = try ht.getJson();
    const obj = json.object;

    try std.testing.expectEqualStrings("my custom detail", obj.get("detail").?.string);
    try std.testing.expectEqualStrings("xreq-999", obj.get("request_id").?.string);
}

// ── T4: error_code is the caller-supplied code, not UNKNOWN_ENTRY.code ────────

test "M11_001: error_code in body is caller-supplied code, not UNKNOWN_ENTRY.code" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    // Pass an unregistered code — fallback to UNKNOWN_ENTRY for status/title,
    // but error_code must still be what the caller passed.
    const unregistered = "UZ-DOES-NOT-EXIST-IN-TABLE";
    common.errorResponse(ht.res, unregistered, "sentinel check", "req-sentinel");

    const json = try ht.getJson();
    const obj = json.object;

    try std.testing.expectEqualStrings(unregistered, obj.get("error_code").?.string);
    // Must NOT be the UNKNOWN_ENTRY sentinel code
    try std.testing.expect(!std.mem.eql(u8, error_table.UNKNOWN_ENTRY.code, obj.get("error_code").?.string));
}

// ── T5: Unregistered code → 500, caller code preserved ───────────────────────

test "M11_001: unregistered code → HTTP 500 fallback" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    common.errorResponse(ht.res, "UZ-DOES-NOT-EXIST-IN-TABLE", "fallback test", "req-fb");
    try ht.expectStatus(500);
}

// ── T2: Edge cases — empty / boundary inputs ─────────────────────────────────

test "M11_001 T2: empty detail string passes through without crashing" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    common.errorResponse(ht.res, error_codes.ERR_UNAUTHORIZED, "", "req-t2a");
    try ht.expectStatus(401);
    const json = try ht.getJson();
    try std.testing.expectEqualStrings("", json.object.get("detail").?.string);
}

test "M11_001 T2: empty request_id string passes through without crashing" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    common.errorResponse(ht.res, error_codes.ERR_UNAUTHORIZED, "some detail", "");
    try ht.expectStatus(401);
    const json = try ht.getJson();
    try std.testing.expectEqualStrings("", json.object.get("request_id").?.string);
}

test "M11_001 T2: very long detail (1000 chars) does not crash" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const long_detail = "x" ** 1000;
    common.errorResponse(ht.res, error_codes.ERR_UNAUTHORIZED, long_detail, "req-t2c");
    try ht.expectStatus(401);
    const json = try ht.getJson();
    try std.testing.expectEqual(@as(usize, 1000), json.object.get("detail").?.string.len);
}

test "M11_001 T2: docs_uri in body matches table entry (not constructed by caller)" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    common.errorResponse(ht.res, error_codes.ERR_UNAUTHORIZED, "d", "r");
    const json = try ht.getJson();
    const expected_entry = error_table.lookup(error_codes.ERR_UNAUTHORIZED).?;
    try std.testing.expectEqualStrings(expected_entry.docs_uri, json.object.get("docs_uri").?.string);
}

test "M11_001 T2: title in body matches table entry (caller cannot override)" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    common.errorResponse(ht.res, error_codes.ERR_UNAUTHORIZED, "d", "r");
    const json = try ht.getJson();
    const expected_entry = error_table.lookup(error_codes.ERR_UNAUTHORIZED).?;
    try std.testing.expectEqualStrings(expected_entry.title, json.object.get("title").?.string);
}

// ── T3: No nested 'error' wrapper — RFC 7807 flat body ───────────────────────

test "M11_001 T3: body has no nested 'error' field (old format regression)" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    common.errorResponse(ht.res, error_codes.ERR_UNAUTHORIZED, "detail", "req-t3");
    const json = try ht.getJson();
    // Old format had .error.code — must not exist in new format
    try std.testing.expectEqual(@as(?std.json.Value, null), json.object.get("error"));
}

test "M11_001 T3: body has no nested 'message' field (old format regression)" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    common.errorResponse(ht.res, error_codes.ERR_UNAUTHORIZED, "detail", "req-t3b");
    const json = try ht.getJson();
    // Old format used .message — replaced by .detail in RFC 7807
    try std.testing.expectEqual(@as(?std.json.Value, null), json.object.get("message"));
}

// ── T11: Memory safety — repeated calls don't leak ───────────────────────────

test "M11_001 T11: 100 errorResponse calls with std.testing.allocator — no leaks" {
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
