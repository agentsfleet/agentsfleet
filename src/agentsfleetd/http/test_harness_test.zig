// Unit tests for the test harness's HTTP message types.
//
// Scope: Request builder state + Response assertion helpers. Tests that touch
// only in-memory state — no server, no DB, no network. `.send()` and
// `TestHarness.start/deinit` are covered by the integration suites (tenant_provider,
// rbac, telemetry, dashboard, fleet_steer, tenant_api_keys, webhook).
//
// Request.init takes *TestHarness but only reads `harness.alloc` unless
// `.send()` is called. Tests build a partial harness with only `alloc` set.

const std = @import("std");
const harness = @import("test_harness.zig");
const message = @import("test_http_message.zig");

const TestHarness = harness.TestHarness;
const Request = harness.Request;
const Response = harness.Response;
const MAX_HEADERS = message.MAX_HEADERS;

/// Build a minimally-initialized TestHarness suitable for Request/Response
/// unit tests. The caller MUST NOT invoke `.send()` or harness lifecycle
/// methods on the returned value — only `alloc` is safely initialized.
fn fakeHarness(alloc: std.mem.Allocator) TestHarness {
    return TestHarness{
        .alloc = alloc,
        // Never started: this fixture makes no outbound call, and arming
        // against a stopped scheduler fails closed.
        .deadline_backend = .{},
        // SAFETY: test fixture; the caller must not drive lifecycle methods.
        .deadline_scheduler = undefined,
        // SAFETY: test fixture; field is populated by the surrounding builder before any read.
        .pool = undefined,
        // SAFETY: test fixture; field is populated by the surrounding builder before any read.
        .session_store = undefined,
        // SAFETY: test fixture; field is populated by the surrounding builder before any read.
        .verifier = undefined,
        // SAFETY: test fixture; field is populated by the surrounding builder before any read.
        .queue = undefined,
        // SAFETY: test fixture; field is populated by the surrounding builder before any read.
        .telemetry = undefined,
        // SAFETY: test fixture; field is populated by the surrounding builder before any read.
        .registry = undefined,
        // SAFETY: test fixture; field is populated by the surrounding builder before any read.
        .ctx = undefined,
        // SAFETY: test fixture; field is populated by the surrounding builder before any read.
        .hub = undefined,
        // SAFETY: test fixture; field is populated by the surrounding builder before any read.
        .streams = undefined,
        // SAFETY: test fixture; field is populated by the surrounding builder before any read.
        .fleet_sets = undefined,
        // SAFETY: test fixture; field is populated by the surrounding builder before any read.
        .server = undefined,
        // SAFETY: test fixture; field is populated by the surrounding builder before any read.
        .thread = undefined,
        .port = 0,
    };
}

fn makeResponse(alloc: std.mem.Allocator, status: u16, body: []const u8) !Response {
    return Response{
        .status = status,
        .body = try alloc.dupe(u8, body),
        .alloc = alloc,
    };
}

test "Response.header reads captured headers case-insensitively" {
    const alloc = std.testing.allocator;
    const body = try alloc.dupe(u8, "{}");
    const headers = try alloc.dupe(u8, "HTTP/1.1 200 OK\r\nETag: \"abc\"\r\n\r\n");
    const response: Response = .{ .status = 200, .body = body, .headers = headers, .alloc = alloc };
    defer response.deinit();
    try std.testing.expectEqualStrings("\"abc\"", response.header("etag").?);
    try std.testing.expect(response.header("missing") == null);
}

// ── T1: Request builder happy paths ────────────────────────────────────────

test "Request.header stores name and value at current slot" {
    var h = fakeHarness(std.testing.allocator);
    const r = try Request.init(&h, .GET, "/foo").header("x-test", "hello");
    try std.testing.expectEqual(@as(usize, 1), r.hdr_count);
    try std.testing.expectEqualStrings("x-test", r.hdr_names[0]);
    try std.testing.expectEqualStrings("hello", r.hdr_values[0]);
}

test "Request.header chained — both headers recorded in order" {
    var h = fakeHarness(std.testing.allocator);
    const r = try (try Request.init(&h, .POST, "/bar").header("a", "1")).header("b", "2");
    try std.testing.expectEqual(@as(usize, 2), r.hdr_count);
    try std.testing.expectEqualStrings("a", r.hdr_names[0]);
    try std.testing.expectEqualStrings("b", r.hdr_names[1]);
    try std.testing.expectEqualStrings("2", r.hdr_values[1]);
}

test "Request.bearer sets authorization header and owns the allocation" {
    const alloc = std.testing.allocator;
    var h = fakeHarness(alloc);
    const r = try Request.init(&h, .GET, "/x").bearer("tok123");
    defer if (r.bearer_owned) |v| alloc.free(v);

    try std.testing.expectEqual(@as(usize, 1), r.hdr_count);
    try std.testing.expectEqualStrings("authorization", r.hdr_names[0]);
    try std.testing.expectEqualStrings("Bearer tok123", r.hdr_values[0]);
    try std.testing.expect(r.bearer_owned != null);
    try std.testing.expectEqualStrings("Bearer tok123", r.bearer_owned.?);
}

test "Request.json adds content-type header and sets body" {
    var h = fakeHarness(std.testing.allocator);
    const body = "{\"k\":1}";
    const r = try Request.init(&h, .POST, "/y").json(body);
    try std.testing.expectEqual(@as(usize, 1), r.hdr_count);
    try std.testing.expectEqualStrings("content-type", r.hdr_names[0]);
    try std.testing.expectEqualStrings("application/json", r.hdr_values[0]);
    try std.testing.expect(r.body != null);
    try std.testing.expectEqualStrings(body, r.body.?);
}

test "Request.rawBody sets body without content-type" {
    var h = fakeHarness(std.testing.allocator);
    const r = Request.init(&h, .PUT, "/z").rawBody("plain text");
    try std.testing.expectEqual(@as(usize, 0), r.hdr_count);
    try std.testing.expect(r.body != null);
    try std.testing.expectEqualStrings("plain text", r.body.?);
}

test "Request.header + json preserves prior headers" {
    var h = fakeHarness(std.testing.allocator);
    const r = try (try Request.init(&h, .POST, "/q").header("x-trace", "abc")).json("{}");
    try std.testing.expectEqual(@as(usize, 2), r.hdr_count);
    try std.testing.expectEqualStrings("x-trace", r.hdr_names[0]);
    try std.testing.expectEqualStrings("content-type", r.hdr_names[1]);
}

// ── T2: Edge cases ─────────────────────────────────────────────────────────

test "Request.header fills up to exactly MAX_HEADERS without error" {
    var h = fakeHarness(std.testing.allocator);
    var r = Request.init(&h, .GET, "/");
    var i: usize = 0;
    while (i < MAX_HEADERS) : (i += 1) {
        r = try r.header("x-fill", "v");
    }
    try std.testing.expectEqual(@as(usize, MAX_HEADERS), r.hdr_count);
}

test "Request.rawBody accepts empty string" {
    var h = fakeHarness(std.testing.allocator);
    const r = Request.init(&h, .POST, "/").rawBody("");
    try std.testing.expect(r.body != null);
    try std.testing.expectEqual(@as(usize, 0), r.body.?.len);
}

// ── T3: Negative / error paths ─────────────────────────────────────────────

test "Request.header returns error.TooManyHeaders when full" {
    var h = fakeHarness(std.testing.allocator);
    var r = Request.init(&h, .GET, "/");
    var i: usize = 0;
    while (i < MAX_HEADERS) : (i += 1) {
        r = try r.header("x", "v");
    }
    try std.testing.expectError(error.TooManyHeaders, r.header("one-too-many", "v"));
}

test "Response.expectStatus returns error.UnexpectedStatus on mismatch" {
    const alloc = std.testing.allocator;
    const r = try makeResponse(alloc, 500, "{}");
    defer r.deinit();
    try std.testing.expectError(error.UnexpectedStatus, r.expectStatus(.ok));
}

test "Response.expectErrorCode returns error.ErrorCodeMismatch when code absent" {
    const alloc = std.testing.allocator;
    const r = try makeResponse(alloc, 401, "{\"detail\":\"no code field\"}");
    defer r.deinit();
    try std.testing.expectError(error.ErrorCodeMismatch, r.expectErrorCode("UZ-WH-010"));
}

// ── T7: Regression — pins review-fix behavior so it can't silently revert ──

test "regression: bearer does not leak when header overflow triggers TooManyHeaders" {
    // Fix 014b1327: bearer() gained `errdefer self.harness.alloc.free(val)` before
    // `try self.header(...)`. Without the errdefer, TooManyHeaders leaks the
    // Bearer string. std.testing.allocator catches the leak if the errdefer
    // is removed.
    const alloc = std.testing.allocator;
    var h = fakeHarness(alloc);
    var r = Request.init(&h, .GET, "/");
    var i: usize = 0;
    while (i < MAX_HEADERS) : (i += 1) {
        r = try r.header("x", "v");
    }
    try std.testing.expectError(error.TooManyHeaders, r.bearer("tok"));
    // Implicit: std.testing.allocator asserts zero leaks at test exit.
}

test "regression: expectErrorCode matches repo's error_code field (not code)" {
    // Fix 494ad1bc: this repo's RFC7807 envelope uses "error_code":"..." not
    // "code":"...". A prior version of expectErrorCode searched for "code":"…"
    // and produced false-positive failures against real 401 responses. This
    // test pins the field name so a naive refactor back to "code":"…" fails.
    const alloc = std.testing.allocator;
    const body =
        \\{"docs_uri":"https://x/y","title":"Unauthorized","detail":"...","error_code":"UZ-AUTH-002","request_id":"req_1"}
    ;
    const r = try makeResponse(alloc, 401, body);
    defer r.deinit();
    try r.expectErrorCode("UZ-AUTH-002");
}

test "Response.bodyContains true when substring present, false otherwise" {
    const alloc = std.testing.allocator;
    const r = try makeResponse(alloc, 200, "{\"status\":\"accepted\",\"event_id\":\"evt_1\"}");
    defer r.deinit();
    try std.testing.expect(r.bodyContains("accepted"));
    try std.testing.expect(r.bodyContains("evt_1"));
    try std.testing.expect(!r.bodyContains("rejected"));
}

test "Response.expectStatus returns void on exact match" {
    const alloc = std.testing.allocator;
    const r = try makeResponse(alloc, 202, "{}");
    defer r.deinit();
    try r.expectStatus(.accepted);
}
