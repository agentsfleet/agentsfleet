//! Integration tests for POST /v1/workspaces.
//!
//! Covers the required-name behaviour:
//!   - missing and blank names return 400 without creating a row
//!   - explicit `{"name": "..."}` succeeds and stores that exact name
//!   - concurrent duplicate explicit names create exactly one row
//!   - missing tenant principal returns 401
//!
//! Requires TEST_DATABASE_URL — skipped gracefully otherwise.

const std = @import("std");
const scope_fixtures = @import("../../test_scope_tokens.zig");
const clock = @import("common").clock;
const pg = @import("pg");
const auth_mw = @import("../../../auth/middleware/mod.zig");
const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const test_fixtures = @import("../../../db/test_fixtures.zig");

const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;

// Reuse the tenant + signed token from tenant_workspaces_integration_test —
// same TEST_JWKS, so the rsa256 signature validates.
const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const TEST_ISSUER = scope_fixtures.ISSUER;
const TEST_AUDIENCE = scope_fixtures.AUDIENCE;
const TEST_JWKS = scope_fixtures.JWKS;
const TOKEN_USER = scope_fixtures.TENANT_ADMIN;
const CONCURRENT_REQUEST_COUNT = 100;
const MIN_PEAK_IN_FLIGHT = 2;
const ERROR_CODE_FRAGMENT = "\"error_code\":\"UZ-WORKSPACE-001\"";
const CURRENT_STATE_FRAGMENT = "\"current_state\":\"name_exists\"";
const SQL_DETAIL_FRAGMENT = "duplicate key";
const OVERLONG_WORKSPACE_NAME = "a" ** 129;
const WORKSPACE_NAME_MAX_CODEPOINTS = 128;
const MULTIBYTE_WORKSPACE_CHARACTER = "🙂";

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn makeHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = TEST_JWKS,
        .issuer = TEST_ISSUER,
        .audience = TEST_AUDIENCE,
    });
}

fn seedTenant(conn: *pg.Conn, _: i64) !void {
    try test_fixtures.seedTenantById(conn, TEST_TENANT_ID, "CreateWsTest");
}

fn countTenantRows(conn: *pg.Conn) !i64 {
    var q = PgQuery.from(try conn.query(
        "SELECT COUNT(*)::BIGINT FROM core.workspaces WHERE tenant_id = $1::uuid",
        .{TEST_TENANT_ID},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return error.MissingCount;
    return row.get(i64, 0);
}

fn multibyteWorkspaceName(
    alloc: std.mem.Allocator,
    codepoint_count: usize,
    suffix: []const u8,
) ![]u8 {
    if (suffix.len > codepoint_count) return error.SuffixTooLong;
    const multibyte_count = codepoint_count - suffix.len;
    const bytes = try alloc.alloc(
        u8,
        multibyte_count * MULTIBYTE_WORKSPACE_CHARACTER.len + suffix.len,
    );
    var offset: usize = 0;
    for (0..multibyte_count) |_| {
        @memcpy(
            bytes[offset..][0..MULTIBYTE_WORKSPACE_CHARACTER.len],
            MULTIBYTE_WORKSPACE_CHARACTER,
        );
        offset += MULTIBYTE_WORKSPACE_CHARACTER.len;
    }
    @memcpy(bytes[offset..], suffix);
    return bytes;
}

test "integration: POST /v1/workspaces validates the workspace name" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedTenant(conn, clock.nowMillis());

    const before = try countTenantRows(conn);
    const missing = try (try (try h.post("/v1/workspaces").bearer(TOKEN_USER)).json("{}")).send();
    defer missing.deinit();
    try missing.expectStatus(.bad_request);

    const blank = try (try (try h.post("/v1/workspaces").bearer(TOKEN_USER)).json(
        \\{"name":"   "}
    )).send();
    defer blank.deinit();
    try blank.expectStatus(.bad_request);

    const control_whitespace = try (try (try h.post("/v1/workspaces").bearer(TOKEN_USER)).json(
        "{\"name\":\"\\f\\v\"}",
    )).send();
    defer control_whitespace.deinit();
    try control_whitespace.expectStatus(.bad_request);

    const unicode_whitespace = try (try (try h.post("/v1/workspaces").bearer(TOKEN_USER)).json(
        "{\"name\":\"\\u00a0\\u2007\\u202f\"}",
    )).send();
    defer unicode_whitespace.deinit();
    try unicode_whitespace.expectStatus(.bad_request);

    const malformed = try (try (try h.post("/v1/workspaces").bearer(TOKEN_USER)).json(
        \\{"name":
    )).send();
    defer malformed.deinit();
    try malformed.expectStatus(.bad_request);

    const null_character = try (try (try h.post("/v1/workspaces").bearer(TOKEN_USER)).json(
        "{\"name\":\"invalid\\u0000name\"}",
    )).send();
    defer null_character.deinit();
    try null_character.expectStatus(.bad_request);
    try std.testing.expect(null_character.bodyContains("\"error_code\":\"UZ-REQ-001\""));
    try std.testing.expect(!null_character.bodyContains(SQL_DETAIL_FRAGMENT));

    const terminal_controls = try (try (try h.post("/v1/workspaces").bearer(TOKEN_USER)).json(
        "{\"name\":\"line\\nbreak\\u001b[31m\"}",
    )).send();
    defer terminal_controls.deinit();
    try terminal_controls.expectStatus(.bad_request);
    try std.testing.expect(terminal_controls.bodyContains("\"error_code\":\"UZ-REQ-001\""));
    try std.testing.expect(!terminal_controls.bodyContains(SQL_DETAIL_FRAGMENT));

    const directional_controls = try (try (try h.post("/v1/workspaces").bearer(TOKEN_USER)).json(
        "{\"name\":\"safe\\u202Etxt\"}",
    )).send();
    defer directional_controls.deinit();
    try directional_controls.expectStatus(.bad_request);
    try std.testing.expect(directional_controls.bodyContains("\"error_code\":\"UZ-REQ-001\""));
    try std.testing.expect(!directional_controls.bodyContains(SQL_DETAIL_FRAGMENT));

    const line_separator = try (try (try h.post("/v1/workspaces").bearer(TOKEN_USER)).json(
        "{\"name\":\"safe\\u2028txt\"}",
    )).send();
    defer line_separator.deinit();
    try line_separator.expectStatus(.bad_request);
    try std.testing.expect(line_separator.bodyContains("\"error_code\":\"UZ-REQ-001\""));
    try std.testing.expect(!line_separator.bodyContains(SQL_DETAIL_FRAGMENT));

    const overlong_body = try std.fmt.allocPrint(
        alloc,
        "{{\"name\":\"{s}\"}}",
        .{OVERLONG_WORKSPACE_NAME},
    );
    defer alloc.free(overlong_body);
    const overlong = try (try (try h.post("/v1/workspaces").bearer(TOKEN_USER)).json(
        overlong_body,
    )).send();
    defer overlong.deinit();
    try overlong.expectStatus(.bad_request);
    try std.testing.expect(overlong.bodyContains("\"error_code\":\"UZ-REQ-001\""));
    try std.testing.expect(!overlong.bodyContains(SQL_DETAIL_FRAGMENT));
    try std.testing.expectEqual(before, try countTenantRows(conn));
}

test "integration: POST /v1/workspaces trims the name and returns identifiers" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedTenant(conn, clock.nowMillis());

    // Use a unique-per-test-run name so re-runs against a persistent DB
    // do not collide with the tenant-scoped workspace-name rule.
    const ts = clock.nowMillis();
    const body = try std.fmt.allocPrint(alloc, "{{\"name\":\"  explicit-{d}  \"}}", .{ts});
    defer alloc.free(body);

    const r = try (try (try h.post("/v1/workspaces").bearer(TOKEN_USER)).json(body)).send();
    defer r.deinit();

    try r.expectStatus(.created);
    const expected = try std.fmt.allocPrint(alloc, "\"name\":\"explicit-{d}\"", .{ts});
    defer alloc.free(expected);
    try std.testing.expect(r.bodyContains(expected));
    try std.testing.expect(r.bodyContains("\"workspace_id\":\""));
    try std.testing.expect(r.bodyContains("\"request_id\":\""));
    try std.testing.expect(r.bodyContains("\"tenant_id\":\""));
}

test "integration: POST /v1/workspaces counts multibyte names by code point" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedTenant(conn, clock.nowMillis());

    const suffix = try std.fmt.allocPrint(alloc, "-{d}", .{clock.nowMillis()});
    defer alloc.free(suffix);
    const accepted_name = try multibyteWorkspaceName(
        alloc,
        WORKSPACE_NAME_MAX_CODEPOINTS,
        suffix,
    );
    defer alloc.free(accepted_name);
    const accepted_body = try std.fmt.allocPrint(
        alloc,
        "{{\"name\":\"{s}\"}}",
        .{accepted_name},
    );
    defer alloc.free(accepted_body);
    const accepted = try (try (try h.post("/v1/workspaces").bearer(TOKEN_USER)).json(accepted_body)).send();
    defer accepted.deinit();
    try accepted.expectStatus(.created);
    try std.testing.expectEqual(@as(i64, 1), try countRowsForName(conn, accepted_name));

    const rejected_name = try multibyteWorkspaceName(
        alloc,
        WORKSPACE_NAME_MAX_CODEPOINTS + 1,
        suffix,
    );
    defer alloc.free(rejected_name);
    const rejected_body = try std.fmt.allocPrint(
        alloc,
        "{{\"name\":\"{s}\"}}",
        .{rejected_name},
    );
    defer alloc.free(rejected_body);
    const rejected = try (try (try h.post("/v1/workspaces").bearer(TOKEN_USER)).json(rejected_body)).send();
    defer rejected.deinit();
    try rejected.expectStatus(.bad_request);
    try std.testing.expectEqual(@as(i64, 0), try countRowsForName(conn, rejected_name));
}

fn countRowsForName(conn: *pg.Conn, name: []const u8) !i64 {
    var q = PgQuery.from(try conn.query(
        \\SELECT COUNT(*)::BIGINT
        \\FROM core.workspaces
        \\WHERE tenant_id = $1::uuid AND name = $2
    , .{ TEST_TENANT_ID, name }));
    defer q.deinit();
    const row = (try q.next()) orelse return error.MissingCount;
    return row.get(i64, 0);
}

test "integration: concurrent duplicate workspace names create exactly one row" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const original_request_limit = h.ctx.api_max_in_flight_requests;
    h.ctx.api_max_in_flight_requests = CONCURRENT_REQUEST_COUNT;
    defer h.ctx.api_max_in_flight_requests = original_request_limit;

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try seedTenant(conn, clock.nowMillis());

    const ts = clock.nowMillis();
    const name = try std.fmt.allocPrint(alloc, "concurrent-{d}", .{ts});
    defer alloc.free(name);
    const body = try std.fmt.allocPrint(alloc, "{{\"name\":\"{s}\"}}", .{name});
    defer alloc.free(body);
    var threads: [CONCURRENT_REQUEST_COUNT]std.Thread = undefined;
    var statuses: [CONCURRENT_REQUEST_COUNT]u16 = .{0} ** CONCURRENT_REQUEST_COUNT;
    var safe_conflicts: [CONCURRENT_REQUEST_COUNT]bool = .{false} ** CONCURRENT_REQUEST_COUNT;
    var ready = std.atomic.Value(usize).init(0);
    var gate = std.atomic.Value(bool).init(false);
    var server_peak = std.atomic.Value(u32).init(0);
    h.ctx.api_peak_in_flight_probe = &server_peak;
    defer h.ctx.api_peak_in_flight_probe = null;
    const Worker = struct {
        fn run(
            harness: *TestHarness,
            request_body: []const u8,
            status: *u16,
            safe_conflict: *bool,
            ready_count: *std.atomic.Value(usize),
            start_gate: *std.atomic.Value(bool),
        ) void {
            _ = ready_count.fetchAdd(1, .acq_rel);
            // safe because: the release store publishes only after every worker is ready.
            while (!start_gate.load(.acquire)) std.atomic.spinLoopHint();
            const response = (harness.post("/v1/workspaces").bearer(TOKEN_USER) catch return)
                .json(request_body) catch return;
            const sent = response.send() catch return;
            defer sent.deinit();
            status.* = sent.status;
            safe_conflict.* = sent.bodyContains(ERROR_CODE_FRAGMENT) and
                sent.bodyContains(CURRENT_STATE_FRAGMENT) and
                !sent.bodyContains(SQL_DETAIL_FRAGMENT);
        }
    };
    var spawned: usize = 0;
    errdefer {
        // safe because: this release unblocks workers before each thread join.
        gate.store(true, .release);
        for (threads[0..spawned]) |thread| thread.join();
    }
    for (&threads, 0..) |*thread, index| {
        thread.* = try std.Thread.spawn(.{}, Worker.run, .{
            h,
            body,
            &statuses[index],
            &safe_conflicts[index],
            &ready,
            &gate,
        });
        spawned += 1;
    }
    // safe because: each worker increments ready before its acquire wait.
    while (ready.load(.acquire) != CONCURRENT_REQUEST_COUNT) std.atomic.spinLoopHint();
    // safe because: this release publishes the start after all workers are ready.
    gate.store(true, .release);
    for (threads) |thread| thread.join();
    spawned = 0;

    var created_count: usize = 0;
    var conflict_count: usize = 0;
    for (statuses, safe_conflicts) |status, safe_conflict| {
        if (status == @intFromEnum(std.http.Status.created)) {
            created_count += 1;
        } else if (status == @intFromEnum(std.http.Status.conflict)) {
            conflict_count += 1;
            try std.testing.expect(safe_conflict);
        } else {
            return error.UnexpectedCreateStatus;
        }
    }
    try std.testing.expectEqual(@as(usize, 1), created_count);
    try std.testing.expectEqual(@as(usize, CONCURRENT_REQUEST_COUNT - 1), conflict_count);
    // safe because: all request threads are joined before the peak is read.
    try std.testing.expect(server_peak.load(.acquire) >= MIN_PEAK_IN_FLIGHT);
    try std.testing.expectEqual(@as(i64, 1), try countRowsForName(conn, name));
}

test "integration: POST /v1/workspaces without auth returns 401" {
    const alloc = std.testing.allocator;
    const h = makeHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const r = try (try h.post("/v1/workspaces").json("{}")).send();
    defer r.deinit();
    try r.expectStatus(.unauthorized);
}
