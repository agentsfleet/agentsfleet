//! A fleet's own name reaches the surfaces that name it to a human.
//!
//! `core.fleets.name` is the instance identity; `config_json`'s name is the
//! bundle's TRIGGER.md declaration, shared by every fleet installed from that
//! bundle. They diverge whenever the create path derives or overrides a name —
//! the second install from one template gets a suffix, and an operator may pass
//! `name` outright. Nothing rewrites `config_json` in either case, because the
//! bundle did not change.
//!
//! So a caller that reads the config's name to identify THIS fleet names the
//! wrong one. The approval card did exactly that: a workspace running two fleets
//! from one bundle got two Slack cards carrying the same name, neither of them
//! the installed fleet's. `FleetSession` now carries the row's name and
//! `approval_gate_park` reads it.

const std = @import("std");
const auth_mw = @import("../auth/middleware/mod.zig");
const harness_mod = @import("../http/test_harness.zig");
const TestHarness = harness_mod.TestHarness;
const base = @import("../db/test_fixtures.zig");
const FleetSession = @import("fleet_session.zig");

const ALLOC = std.testing.allocator;

// UUIDv7 literals (version nibble 7, variant 8) so the schema id CHECK passes.
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0df011";
const FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dfc01";
const STOPPED_FLEET_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0dfc02";

/// What the bundle's TRIGGER.md declares — every install from it says this.
const BUNDLE_NAME = "github-pr-reviewer";
/// What the server stored for the SECOND install, the taken default suffixed.
const INSTANCE_NAME = "github-pr-reviewer-042";

const CONFIG_JSON =
    \\{"name":"github-pr-reviewer","x-agentsfleet":{"triggers":[{"type":"webhook","source":"agentmail"}],"tools":["agentmail"],"budget":{"daily_dollars":5.0}}}
;
const SOURCE_MD =
    \\---
    \\name: github-pr-reviewer
    \\---
    \\
    \\You review pull requests.
;

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn startHarness() !*TestHarness {
    return TestHarness.start(ALLOC, .{ .configureRegistry = configureRegistry });
}

test "integration: a claimed fleet carries its own name, not the bundle's" {
    const h = startHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    _ = try conn.exec("DELETE FROM core.fleets WHERE id = $1::uuid", .{FLEET_ID});
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    // The divergence the create path produces: the row was named by the server,
    // the config still declares what the bundle always declared.
    try base.seedFleet(conn, FLEET_ID, WORKSPACE_ID, INSTANCE_NAME, CONFIG_JSON, SOURCE_MD);

    var session = try FleetSession.claimFleet(ALLOC, FLEET_ID, h.pool);
    defer session.deinit(ALLOC);

    try std.testing.expectEqualStrings(INSTANCE_NAME, session.name);
    // The config keeps the bundle's name — it describes the bundle, and nothing
    // rewrote it. Both are loaded; the caller picks by what it is naming.
    try std.testing.expectEqualStrings(BUNDLE_NAME, session.config.name);

    _ = try conn.exec("DELETE FROM core.fleets WHERE id = $1::uuid", .{FLEET_ID});
}

test "integration: claiming a stopped fleet fails without leaking what it had loaded" {
    const h = startHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    _ = try conn.exec("DELETE FROM core.fleets WHERE id = $1::uuid", .{STOPPED_FLEET_ID});
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WORKSPACE_ID);
    try base.seedFleetWithStatus(conn, STOPPED_FLEET_ID, WORKSPACE_ID, "stopped-fleet", "stopped");

    // The status check sits AFTER the row's owned copies are taken, so this is
    // the path every one of those errdefers exists for. `std.testing.allocator`
    // is the assertion: it fails the test on any byte the early return orphans,
    // which is the only proof the errdefer chain is actually correct.
    try std.testing.expectError(error.FleetNotActive, FleetSession.claimFleet(ALLOC, STOPPED_FLEET_ID, h.pool));

    _ = try conn.exec("DELETE FROM core.fleets WHERE id = $1::uuid", .{STOPPED_FLEET_ID});
}
