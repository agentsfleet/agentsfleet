// Fixtures for the multiplexed workspace-stream suites (integration + soak).
//
// Builds on the per-fleet SSE fixtures (`fleets/sse_test_fixtures.zig`): the
// operator token, JWKS, and the tenant/workspace rows its claims reference are
// reused verbatim (RULE TFX — one declaration site for the persona). This file
// adds only what the workspace fan-in needs that the per-fleet suite does not:
// a SECOND workspace in the same tenant (for the isolation test — a frame on
// its fleet must never reach a stream scoped to the first) and the multiplexed
// stream path.

const std = @import("std");
const pg = @import("pg");
const common = @import("common");
const clock = common.clock;

const sse_fixtures = @import("../fleets/sse_test_fixtures.zig");

// Re-export the per-fleet persona so the workspace suites read from one place.
pub const TEST_TENANT_ID = sse_fixtures.TEST_TENANT_ID;
pub const TEST_WORKSPACE_ID = sse_fixtures.TEST_WORKSPACE_ID;
pub const TOKEN_OPERATOR = sse_fixtures.TOKEN_OPERATOR;
pub const SUBSCRIBE_SETTLE_NS = sse_fixtures.SUBSCRIBE_SETTLE_NS;
pub const startHarnessWithWorkspace = sse_fixtures.startHarnessWithWorkspace;
pub const seedFleet = sse_fixtures.seedFleet;
pub const connectPublisher = sse_fixtures.connectPublisher;
pub const activityChannel = sse_fixtures.activityChannel;
pub const cleanupFleet = sse_fixtures.cleanupFleet;
pub const closeAndWakeSubscriber = sse_fixtures.closeAndWakeSubscriber;

/// A second workspace in the SAME tenant. The operator token is scope-pinned to
/// TEST_WORKSPACE_ID, so a stream can never be opened against this one — but a
/// fleet seeded here lets the isolation test publish on a channel outside the
/// authorized set and prove it never arrives.
pub const OTHER_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f22";

/// A workspace id that resolves to no row — the "unauthorized workspace" case.
pub const UNKNOWN_WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f99";

pub fn seedOtherWorkspace(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO workspaces (workspace_id, tenant_id, created_at)
        \\VALUES ($1, $2, $3) ON CONFLICT (workspace_id) DO NOTHING
    , .{ OTHER_WORKSPACE_ID, TEST_TENANT_ID, clock.nowMillis() });
}

/// A fleet in a specific workspace (the base fixture seeds only into
/// TEST_WORKSPACE_ID).
pub fn seedFleetInWorkspace(conn: *pg.Conn, workspace_id: []const u8, fleet_id: []const u8, name: []const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO core.fleets (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, $3, '---\nname: zz\n---\ntest', '{"name":"zz"}', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ fleet_id, workspace_id, name });
}

/// The multiplexed stream path for the operator's workspace.
pub fn workspaceStreamPath(alloc: std.mem.Allocator) ![]u8 {
    return std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/events/stream", .{TEST_WORKSPACE_ID});
}

/// The multiplexed stream path for an arbitrary workspace (the forbidden case).
pub fn workspaceStreamPathFor(alloc: std.mem.Allocator, workspace_id: []const u8) ![]u8 {
    return std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/events/stream", .{workspace_id});
}
