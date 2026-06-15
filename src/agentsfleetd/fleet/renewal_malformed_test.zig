// Failure-path integration tests for the `/renew` HTTP route's ownership +
// existence guards. Drives POST /v1/runners/me/leases/{lease_id}/renew through
// the real router + runner_bearer middleware against the live test DB. Every
// case here is a refusal the runner must distinguish from a transient error:
//   - a missing path param → the route never matches (404, no lease).
//   - a valid runner token but a lease_id that is absent in the DB → 404
//     UZ-RUN-006 (lease_not_found): the load returns null before any extend.
//   - a lease that exists but is owned by another runner → the presenting
//     runner's id-scoped load returns null too (ownership is the runner_id
//     scope), so a non-owner sees the same 404 UZ-RUN-006 — never the lease.
//
// Ownership is scoped to the presenting runner: `loadLease` filters on
// `runner_id = $2`, so a foreign lease is indistinguishable from a missing one
// (no information leak about another runner's lease). Requires LIVE_DB=1 +
// Redis; skipped via TestHarness.start when either is missing.

const std = @import("std");
const clock = @import("common").clock;
const pg = @import("pg");
const auth_mw = @import("../auth/middleware/mod.zig");
const serve_runner_lookup = @import("../cmd/serve_runner_lookup.zig");
const api_key = @import("../auth/api_key.zig");
const harness_mod = @import("../http/test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const protocol = @import("contract").protocol;
const base = @import("../db/test_fixtures.zig");

const ALLOC = std.testing.allocator;

// UUIDv7 literals (version nibble 7, variant 8) so the schema id CHECKs pass.
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d9011";
const RUNNER_A_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d9a01";
const RUNNER_B_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d9b01";
const AGENTSFLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d9c01";
const LEASE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d9f01";
// A well-formed UUID that is never inserted — the unknown-lease case.
const ABSENT_LEASE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0d9fff";
const RUNNER_A_TOKEN = "zrn_" ++ "d" ** 64;
const RUNNER_B_TOKEN = "zrn_" ++ "e" ** 64;

// The real DB-backed runner lookup, parked at module scope so the value outlives
// the middleware chain (tests run sequentially in one process).
// SAFETY: populated by configureRegistry before the chain reads it.
var runner_lookup_ctx: serve_runner_lookup.Ctx = undefined;

fn configureRegistry(reg: *auth_mw.MiddlewareRegistry, h: *TestHarness) anyerror!void {
    runner_lookup_ctx = .{ .pool = h.pool };
    reg.runner_bearer_mw = .{ .host = &runner_lookup_ctx, .lookup = serve_runner_lookup.lookup };
}

fn seedRunner(conn: *pg.Conn, runner_id: []const u8, host_id: []const u8, token: []const u8) !void {
    const hash = api_key.sha256Hex(token);
    _ = try conn.exec(
        \\INSERT INTO fleet.runners
        \\  (id, host_id, token_hash, sandbox_tier, admin_state, labels, tenant_id,
        \\   last_seen_at, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, 'dev_none', 'active', '[]'::jsonb, NULL, 0, 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ runner_id, host_id, hash[0..] });
}

/// Seed an active lease owned by `runner_id`, valid into the future so the
/// status/credit checks could not be the refusal reason — only ownership is.
fn seedActiveLease(conn: *pg.Conn, runner_id: []const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO fleet.runner_leases
        \\  (id, runner_id, zombie_id, workspace_id, tenant_id, event_id, actor,
        \\   event_type, request_json, event_created_at, posture, provider, model,
        \\   metered_input_tokens, metered_cached_tokens, metered_output_tokens, last_metered_at_ms,
        \\   fencing_token, lease_expires_at, status, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4::uuid, $5::uuid, 'evt-renew-mal-1',
        \\        'steer:test', 'chat', '{"message":"hi"}', 0, 'platform',
        \\        'test-provider', 'test-model', 0, 0, 0, 0, 1, $6, 'active', 0, 0)
        \\ON CONFLICT (id) DO NOTHING
    , .{ LEASE_ID, runner_id, AGENTSFLEET_ID, WORKSPACE_ID, base.TEST_TENANT_ID, clock.nowMillis() + 60_000 });
}

fn renewPath(alloc: std.mem.Allocator, lease_id: []const u8) ![]const u8 {
    return std.fmt.allocPrint(alloc, "{s}/{s}/{s}", .{
        protocol.PATH_RUNNER_LEASES, lease_id, protocol.RUNNER_LEASE_RENEW_SUFFIX,
    });
}

fn renewAs(h: *TestHarness, token: []const u8, lease_id: []const u8) !harness_mod.Response {
    const path = try renewPath(ALLOC, lease_id);
    defer ALLOC.free(path);
    const req = try (try h.post(path).bearer(token)).json("{}");
    return req.send();
}

fn execIgnore(conn: *pg.Conn, sql: []const u8, args: anytype) void {
    _ = conn.exec(sql, args) catch |err| std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
}

fn cleanupAll(conn: *pg.Conn) void {
    execIgnore(conn, "DELETE FROM fleet.runner_leases WHERE id = $1::uuid", .{LEASE_ID});
    execIgnore(conn, "DELETE FROM fleet.runners WHERE id IN ($1::uuid, $2::uuid)", .{ RUNNER_A_ID, RUNNER_B_ID });
    base.teardownTenant(conn);
    base.teardownWorkspace(conn, WORKSPACE_ID);
}

fn startHarness() !*TestHarness {
    return TestHarness.start(ALLOC, .{ .configureRegistry = configureRegistry }) catch |err| {
        if (err == error.SkipZigTest) return error.SkipZigTest;
        return err;
    };
}

test "renew without a lease_id path segment 404s when the route never matches" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    cleanupAll(conn);
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try seedRunner(conn, RUNNER_A_ID, "renew-mal-a", RUNNER_A_TOKEN);
    defer cleanupAll(conn);

    // No lease_id segment + no trailing renew suffix → the renew matcher cannot
    // bind a lease and the router returns 404. The runner gets no lease back.
    const req = try (try h.post(protocol.PATH_RUNNER_LEASES ++ "//" ++ protocol.RUNNER_LEASE_RENEW_SUFFIX).bearer(RUNNER_A_TOKEN)).json("{}");
    const resp = try req.send();
    defer resp.deinit();
    try resp.expectStatus(.not_found);
}

test "renew with a well-formed but unknown lease_id is refused 404 UZ-RUN-006" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    cleanupAll(conn);
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try seedRunner(conn, RUNNER_A_ID, "renew-mal-a", RUNNER_A_TOKEN);
    defer cleanupAll(conn);

    // Valid runner token, but ABSENT_LEASE_ID was never inserted: loadLease
    // returns null and the handler answers lease_not_found before any extend.
    const resp = try renewAs(h, RUNNER_A_TOKEN, ABSENT_LEASE_ID);
    defer resp.deinit();
    try resp.expectStatus(.not_found);
    try resp.expectErrorCode("UZ-RUN-006");
}

test "renew by a runner that is not the lease owner is refused 404 UZ-RUN-006" {
    const h = try startHarness();
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    cleanupAll(conn);
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try seedRunner(conn, RUNNER_A_ID, "renew-mal-a", RUNNER_A_TOKEN); // owner
    try seedRunner(conn, RUNNER_B_ID, "renew-mal-b", RUNNER_B_TOKEN); // interloper
    try seedActiveLease(conn, RUNNER_A_ID); // lease belongs to A
    defer cleanupAll(conn);

    // Runner B presents a valid token but does not own LEASE_ID. The id-scoped
    // load (runner_id = B) finds no row → the same 404 UZ-RUN-006 a missing
    // lease yields. Ownership scoping never leaks another runner's lease.
    const resp = try renewAs(h, RUNNER_B_TOKEN, LEASE_ID);
    defer resp.deinit();
    try resp.expectStatus(.not_found);
    try resp.expectErrorCode("UZ-RUN-006");
}
