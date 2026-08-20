//! Pool-acquire arms: what a handler answers when the database pool has no
//! connection left to give.
//!
//! Every one of these arms is the code an operator meets during an incident —
//! the moment the daemon is busiest and least observed. None of them had ever
//! run: an ordinary integration test always finds a free connection, so the
//! `catch` beside each `pool.acquire()` is dead weight until something starves
//! the pool. The starvation is induced here rather than simulated, so what is
//! asserted is the real arm on the real handler.
//!
//! The pool's acquire timeout (2s by default) bounds each request, so a test
//! here costs seconds, not milliseconds. That is the price of driving the arm
//! through the handler instead of calling it directly, and calling it directly
//! would prove only that it compiles.
//!
//! Requires DATABASE_URL (or TEST_DATABASE_URL) — skipped otherwise.

const std = @import("std");
const pg = @import("pg");

const base = @import("secrets_json_integration_test.zig");
const harness_mod = @import("test_harness.zig");
const ec = @import("../errors/error_registry.zig");

const TestHarness = harness_mod.TestHarness;
const ALLOC = std.testing.allocator;

/// A well-formed UUIDv7 that names no row. The pool arms sit BEHIND id-shape
/// validation and AHEAD of the lookup, so the id must parse to reach them
/// while never needing to exist.
const ABSENT_KEY_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0af001";

const Held = std.ArrayListUnmanaged(*pg.Conn);

/// Take every connection the pool will give, so the next acquire cannot be
/// served.
///
/// Size-agnostic on purpose: the pool size is env-tunable, and a hardcoded
/// count would quietly stop exhausting anything the day the default moves —
/// the tests would keep passing while asserting nothing. Draining until the
/// pool refuses costs one acquire timeout, once.
fn drainPool(h: *TestHarness, held: *Held) !void {
    while (true) {
        const conn = h.acquireConn() catch break;
        try held.append(ALLOC, conn);
    }
    // A pool that handed back nothing was already starved by something else,
    // which would make every assertion below vacuously true.
    try std.testing.expect(held.items.len > 0);
}

fn releaseAll(h: *TestHarness, held: *Held) void {
    for (held.items) |conn| h.releaseConn(conn);
    held.deinit(ALLOC);
}

test "integration: test_pool_exhaustion_answers_unavailable — every acquiring handler answers 503" {
    base.setTestEncryptionKey();
    const h = base.seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    var held: Held = .empty;
    try drainPool(h, &held);
    defer releaseAll(h, &held);

    const prefs_path = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/preferences", .{base.TEST_WS_ID});
    defer ALLOC.free(prefs_path);
    const key_path = try std.fmt.allocPrint(ALLOC, "/v1/api-keys/{s}", .{ABSENT_KEY_ID});
    defer ALLOC.free(key_path);

    { // a read whose first database touch is the acquire
        const r = try (try h.get(prefs_path).bearer(base.TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.service_unavailable);
        try r.expectErrorCode(ec.ERR_INTERNAL_DB_UNAVAILABLE);
    }
    { // a write whose body and name both validate, so the acquire is what fails
        const r = try (try (try h.post("/v1/api-keys").bearer(base.TOKEN_OPERATOR)).json("{\"key_name\":\"pool-starved\"}")).send();
        defer r.deinit();
        try r.expectStatus(.service_unavailable);
        try r.expectErrorCode(ec.ERR_INTERNAL_DB_UNAVAILABLE);
    }
    { // the patch verb reaches its own acquire past id and body validation
        const r = try (try (try h.patch(key_path).bearer(base.TOKEN_OPERATOR)).json("{\"active\":false}")).send();
        defer r.deinit();
        try r.expectStatus(.service_unavailable);
        try r.expectErrorCode(ec.ERR_INTERNAL_DB_UNAVAILABLE);
    }
    { // and the delete verb reaches its own
        const r = try (try h.delete(key_path).bearer(base.TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.service_unavailable);
        try r.expectErrorCode(ec.ERR_INTERNAL_DB_UNAVAILABLE);
    }
}

test "integration: test_pool_recovers_when_connections_return — starvation is not a latch" {
    // The arms above must be a transient answer, not a state the daemon gets
    // stuck in. A pool that never recovers turns one slow query into a
    // permanent outage, and nothing else in the suite would notice: every
    // other test starts from a healthy pool and never starves it.
    base.setTestEncryptionKey();
    const h = base.seedAndHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const prefs_path = try std.fmt.allocPrint(ALLOC, "/v1/workspaces/{s}/preferences", .{base.TEST_WS_ID});
    defer ALLOC.free(prefs_path);

    var held: Held = .empty;
    // Released explicitly below; this catches the drain failing part-way, where
    // the connections taken so far would otherwise be stranded.
    defer releaseAll(h, &held);
    try drainPool(h, &held);
    {
        const r = try (try h.get(prefs_path).bearer(base.TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.service_unavailable);
    }

    // Hand every connection back; the very next request must be served.
    releaseAll(h, &held);
    held = .empty;

    // Asserted through the key delete rather than the preference read: a
    // served preference read still needs a `core.users` row this fixture does
    // not seed, so it would answer 403 and prove nothing about the pool. The
    // delete answers 404 for an id that does not exist — an answer only a
    // handler holding a real connection can give.
    const key_path = try std.fmt.allocPrint(ALLOC, "/v1/api-keys/{s}", .{ABSENT_KEY_ID});
    defer ALLOC.free(key_path);
    const r = try (try h.delete(key_path).bearer(base.TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.not_found);
}
