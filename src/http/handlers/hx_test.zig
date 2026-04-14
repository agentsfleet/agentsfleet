// Unit tests for Hx response helpers.
//
// After M18_002's full sweep, every handler writes responses via
// `hx.ok(status, body)` and `hx.fail(code, detail)`. These tests exercise
// the two methods directly using httpz.testing so regressions in the JSON
// envelope, RFC 7807 shape, or HTTP status plumbing are caught at unit level
// (without spinning up a live HTTP server).
//
// Tiers covered:
//   T1 — happy path: hx.ok writes JSON body at given status
//   T2 — status variants: .ok, .created, .accepted, .service_unavailable
//   T3 — negative: hx.fail with an unregistered code falls back to 500
//   T12 — contract: RFC 7807 body has all required fields; Content-Type is
//         application/problem+json; caller-supplied error_code preserved
//
// These tests intentionally do NOT construct a full Hx — they only need the
// response half. A helper builds a minimal Hx with `ctx: undefined` and
// `principal: undefined` since ok/fail never read those fields. If a future
// change makes ok/fail read ctx or principal, these tests will crash loudly
// (good — the coupling is then visible).

const std = @import("std");
const httpz = @import("httpz");
const common = @import("common.zig");
const hx_mod = @import("hx.zig");
const error_codes = @import("../../errors/error_registry.zig");

const Hx = hx_mod.Hx;

fn buildHx(res: *httpz.Response, req_id: []const u8) Hx {
    return Hx{
        .alloc = std.testing.allocator,
        // ok/fail never read these — if that changes, this test crashes and
        // surfaces the coupling.
        .principal = undefined,
        .req_id = req_id,
        .ctx = undefined,
        .res = res,
    };
}

// ── T1: hx.ok writes JSON at given status ───────────────────────────────────

test "Hx.ok writes 200 and JSON body" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const hx = buildHx(ht.res, "req-ok-1");
    hx.ok(.ok, .{ .status = "ok", .service = "zombied" });

    try ht.expectStatus(200);
    const json = try ht.getJson();
    try std.testing.expectEqualStrings("ok", json.object.get("status").?.string);
    try std.testing.expectEqualStrings("zombied", json.object.get("service").?.string);
}

// ── T2: status code variants ───────────────────────────────────────────────

test "Hx.ok writes 201 Created" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const hx = buildHx(ht.res, "req-ok-2");
    hx.ok(.created, .{ .id = "abc-123" });

    try ht.expectStatus(201);
    const json = try ht.getJson();
    try std.testing.expectEqualStrings("abc-123", json.object.get("id").?.string);
}

test "Hx.ok writes 202 Accepted" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const hx = buildHx(ht.res, "req-ok-3");
    hx.ok(.accepted, .{ .status = "accepted", .event_id = "evt_001" });

    try ht.expectStatus(202);
    const json = try ht.getJson();
    try std.testing.expectEqualStrings("evt_001", json.object.get("event_id").?.string);
}

test "Hx.ok writes 503 Service Unavailable for degraded /readyz" {
    // Readyz uses hx.ok with .service_unavailable when a dependency is down.
    // This test pins that path — a refactor that forces only 2xx through ok()
    // would break /readyz behavior.
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const hx = buildHx(ht.res, "req-ok-4");
    hx.ok(.service_unavailable, .{ .ready = false, .database = true, .queue = false });

    try ht.expectStatus(503);
    const json = try ht.getJson();
    try std.testing.expect(!json.object.get("ready").?.bool);
    try std.testing.expect(!json.object.get("queue").?.bool);
}

// ── T12: hx.fail produces RFC 7807 body with correct HTTP status ──────────

test "Hx.fail with UZ-AUTH-002 → HTTP 401" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const hx = buildHx(ht.res, "req-fail-1");
    hx.fail(error_codes.ERR_UNAUTHORIZED, "missing bearer token");

    try ht.expectStatus(401);
}

test "Hx.fail sets Content-Type application/problem+json" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const hx = buildHx(ht.res, "req-fail-2");
    hx.fail(error_codes.ERR_INVALID_REQUEST, "bad input");

    try ht.expectHeader("Content-Type", "application/problem+json");
}

test "Hx.fail body has all RFC 7807 fields: docs_uri, title, detail, error_code, request_id" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const hx = buildHx(ht.res, "req-fail-3");
    hx.fail(error_codes.ERR_FORBIDDEN, "denied");

    const json = try ht.getJson();
    const obj = json.object;
    try std.testing.expect(obj.get("docs_uri") != null);
    try std.testing.expect(obj.get("title") != null);
    try std.testing.expect(obj.get("detail") != null);
    try std.testing.expect(obj.get("error_code") != null);
    try std.testing.expect(obj.get("request_id") != null);
}

test "Hx.fail passes detail and hx.req_id through verbatim" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const hx = buildHx(ht.res, "xreq-passthrough");
    hx.fail(error_codes.ERR_UNAUTHORIZED, "some custom detail string");

    const json = try ht.getJson();
    try std.testing.expectEqualStrings("some custom detail string", json.object.get("detail").?.string);
    try std.testing.expectEqualStrings("xreq-passthrough", json.object.get("request_id").?.string);
}

// ── T3: negative — unregistered code falls back to 500 ────────────────────

test "Hx.fail with unregistered code → HTTP 500 fallback" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const hx = buildHx(ht.res, "req-fallback");
    hx.fail("UZ-NEVER-REGISTERED-XXX", "sentinel");

    try ht.expectStatus(500);
}

test "Hx.fail with unregistered code preserves caller's code in body (does not substitute UNKNOWN)" {
    // Regression guard: if the registry lookup fell back to UNKNOWN and the
    // fail() helper also substituted UNKNOWN.code into error_code, the caller
    // would lose visibility into what they actually tried to emit.
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const hx = buildHx(ht.res, "req-unknown");
    const unregistered = "UZ-TEST-UNREGISTERED-CODE-9999";
    hx.fail(unregistered, "sentinel");

    const json = try ht.getJson();
    try std.testing.expectEqualStrings(unregistered, json.object.get("error_code").?.string);
    try std.testing.expect(!std.mem.eql(u8, error_codes.UNKNOWN.code, json.object.get("error_code").?.string));
}

// ── T11: no-allocator-leaks ────────────────────────────────────────────────
//
// Hx.ok and Hx.fail themselves don't allocate — they write to res.buffer which
// httpz owns. This test documents that contract: running them under
// std.testing.allocator (which detects leaks) doesn't report any. If a future
// change adds allocation inside ok/fail without matching cleanup, this test
// starts failing.

test "Hx.ok and Hx.fail do not leak — safe under std.testing.allocator" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    const hx = buildHx(ht.res, "req-leak-check");
    hx.ok(.ok, .{ .a = 1, .b = "two" });
    hx.fail(error_codes.ERR_INVALID_REQUEST, "repeat ok call above");
    // std.testing.allocator asserts at test-exit; absence of a leak report
    // is the success condition here.
}
