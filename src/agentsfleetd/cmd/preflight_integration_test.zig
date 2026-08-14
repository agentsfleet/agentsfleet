//! Integration tests for the boot preflight against live datastores.
//!
//! The unit siblings in `preflight_test.zig` prove the injected-env parsing
//! and the ownership contracts; what needs a real database is the other half
//! of each helper — the pool that actually connects, the migration check that
//! reads real schema state, and the credential-broker install whose vault
//! reads land on real (empty) rows and must degrade closed rather than crash.
//!
//! Requires TEST_DATABASE_URL — skipped gracefully otherwise.

const std = @import("std");
const constants = @import("common");
const call_deadline = @import("call_deadline");

const preflight = @import("preflight.zig");
const fixtures = @import("../http/handlers/auth/cli_credentials_test_fixtures.zig");
const credential_broker = @import("../credentials/broker.zig");

const ALLOC = std.testing.allocator;

/// The live database URL the integration lane exports (borrowed from the
/// process environ — nothing to free), or null → skip.
const DB_URL_ENV: [:0]const u8 = "TEST_DATABASE_URL";

test "integration: connectDbPool reaches the live database and checkMigrations passes on a migrated schema" {
    const url = constants.env.testLiveValue(DB_URL_ENV) orelse return error.SkipZigTest;

    var env = try constants.env.fromPairs(ALLOC, &.{.{ "DATABASE_URL_API", url }});
    defer env.deinit();

    const pool = try preflight.connectDbPool(constants.globalIo(), &env, ALLOC, .api);
    defer pool.deinit();

    // The lane migrates before running, so the happy path is the truthful one
    // here: a pending or failed state would be the lane's own defect.
    try preflight.checkMigrations(constants.globalIo(), &env, ALLOC, pool, false);
}

test "integration: installCredentialBroker degrades closed on an empty admin workspace id" {
    const url = constants.env.testLiveValue(DB_URL_ENV) orelse return error.SkipZigTest;
    var env = try constants.env.fromPairs(ALLOC, &.{.{ "DATABASE_URL_API", url }});
    defer env.deinit();
    const pool = try preflight.connectDbPool(constants.globalIo(), &env, ALLOC, .api);
    defer pool.deinit();

    // One process scheduler, as the daemon root owns (stop-signal → join → deinit).
    var backend: call_deadline.MonotonicBackend = .{};
    var sched = call_deadline.ProcessScheduler.init(ALLOC, &backend);
    try sched.start();
    defer sched.deinit();

    var broker_out: ?*credential_broker = null;
    var slug_out: ?[]const u8 = null;
    var handle = preflight.installCredentialBroker(
        ALLOC,
        constants.globalIo(),
        &sched,
        pool,
        "",
        &broker_out,
        &slug_out,
    );
    defer handle.deinit();

    // No workspace to read platform keys from: every integration is
    // unconfigured, the connect slug is absent, and the broker still installs
    // — serving `static` only, never crashing the mint endpoint.
    try std.testing.expect(broker_out != null);
    try std.testing.expect(slug_out == null);
    try std.testing.expect(handle.github_app == null);
    try std.testing.expect(handle.zoho_app == null);
}

test "integration: installCredentialBroker survives a workspace whose vault holds no platform keys" {
    const h = fixtures.seededHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // One process scheduler, as the daemon root owns (stop-signal → join → deinit).
    var backend: call_deadline.MonotonicBackend = .{};
    var sched = call_deadline.ProcessScheduler.init(ALLOC, &backend);
    try sched.start();
    defer sched.deinit();

    var broker_out: ?*credential_broker = null;
    var slug_out: ?[]const u8 = null;
    // A REAL workspace row with an empty vault: every loadJson misses, each
    // integration logs unconfigured and returns null, and the broker still
    // boots. This is the fresh-deployment shape — platform keys arrive later,
    // by hand, and the boot path must not depend on them.
    var handle = preflight.installCredentialBroker(
        ALLOC,
        constants.globalIo(),
        &sched,
        h.pool,
        fixtures.WORKSPACE_ID,
        &broker_out,
        &slug_out,
    );
    defer handle.deinit();

    try std.testing.expect(broker_out != null);
    try std.testing.expect(slug_out == null);
    try std.testing.expect(handle.github_app == null);
    try std.testing.expect(handle.jira_app == null);
    try std.testing.expect(handle.linear_app == null);

    fixtures.cleanup(h);
}
