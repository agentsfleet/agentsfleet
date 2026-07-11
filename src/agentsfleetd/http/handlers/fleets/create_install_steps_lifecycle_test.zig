//! Integration proof for the tracked install worker's lifecycle vs teardown.
//!
//! The production property (serve.zig's teardown order): `spawn` registers on
//! the WaitGroup BEFORE the thread starts and `finish()` is the worker's LAST
//! act — after its final pool touch — so a teardown that `wait()`s on the group
//! never frees the pool/queue under a live worker. Before the worker was
//! tracked, an in-flight worker at shutdown raced the pool teardown
//! (use-after-free on the borrowed pool). This test drives the real detached
//! worker against live Postgres +
//! Redis and uses `wg.wait()` as the only synchronization — the production
//! barrier itself, zero test-side sleeps or polling.
//!
//! DB+Redis-gated: skips without TEST_DATABASE_URL / a reachable Redis.

const std = @import("std");
const testing = std.testing;
const constants = @import("common");
const base = @import("../../../db/test_fixtures.zig");
const harness_mod = @import("../../test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const auth_mw = @import("../../../auth/middleware/mod.zig");
const install_steps = @import("create_install_steps.zig");
const config_types = @import("../../../fleet_runtime/config_types.zig");

// Suite-private ids (uuidv7 version nibble) — never collide with other suites.
const LC_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0acd01";
const LC_FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0acd02";
const LC_FLEET_NAME = "install-lifecycle-probe";

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn seedInstallingFleet(h: *TestHarness) !void {
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, LC_WORKSPACE_ID);
    try base.seedFleetWithStatus(
        conn,
        LC_FLEET_ID,
        LC_WORKSPACE_ID,
        LC_FLEET_NAME,
        config_types.FleetStatus.installing.toSlice(),
    );
}

fn cleanupFleet(h: *TestHarness) void {
    const conn = h.acquireConn() catch return;
    defer h.releaseConn(conn);
    base.teardownFleets(conn, LC_WORKSPACE_ID);
    // Delete the seeded workspace too (fleets first — no cascade on workspace_id).
    // A leftover core.workspaces row under the shared test tenant poisons sibling
    // suites: secret_probe.resolvePrimaryWorkspace bridges tenant→workspace by
    // earliest (created_at ASC, workspace_id ASC), and every fixture is created_at=0,
    // so this low-UUID workspace would win the tie-break and misdirect their
    // credential loads to the wrong workspace (SecretMissing / NotFound).
    base.teardownWorkspace(conn, LC_WORKSPACE_ID);
}

test "integration(install_worker): teardown wait() awaits the in-flight worker; flip lands before finish" {
    const h = TestHarness.start(testing.allocator, .{
        .configureRegistry = configureRegistry,
    }) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    if (!h.tryConnectRedis()) return error.SkipZigTest;

    try seedInstallingFleet(h);
    defer cleanupFleet(h);

    // Production configuration: a real detached worker on the boot-owned pool +
    // queue, tracked by the drain WaitGroup exactly as serve.zig passes it.
    var wg: constants.WaitGroup = .{};
    try install_steps.spawn(h.pool, &h.queue, LC_WORKSPACE_ID, LC_FLEET_ID, &wg);

    // The teardown barrier under test. If spawn registered late (after the
    // thread started) or the worker finished early (before its last pool
    // touch), this returns while the worker still borrows the pool — and the
    // h.deinit() above then frees it under a live thread (UAF caught by
    // testing.allocator / the valgrind lane).
    wg.wait();

    // finish() came after the worker's pool work: the guarded installing→active
    // flip is already visible.
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    const status = (try base.fleetStatusOwned(conn, testing.allocator, LC_FLEET_ID)) orelse
        return error.TestUnexpectedResult;
    defer testing.allocator.free(status);
    try testing.expectEqualStrings(config_types.FleetStatus.active.toSlice(), status);
}
