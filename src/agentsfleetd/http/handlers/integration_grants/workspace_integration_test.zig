// The workspace integration-grant verbs, both of them, past the one arm that
// was reached.
//
// Only the cross-workspace 404 was ever driven (by the IDOR suite), so the
// handler's own body — the listing query, the row copies, the revoke statement
// and every refusal that is not the foreign-fleet case — read dark. That left
// the two answers this endpoint exists to give completely unproven: what a
// grant list contains, and whether a revoke actually revokes.
//
// The revoke statement is scoped by workspace in SQL as well as in the handler,
// so the double-revoke case matters twice: it proves the verb is idempotent for
// the dashboard's retry, and it proves the `status != revoked` guard is what
// refuses the second call rather than the row silently flipping again.
//
// DB-backed: needs TEST_DATABASE_URL — skipped gracefully otherwise.

const std = @import("std");

const auth_mw = @import("../../../auth/middleware/mod.zig");
const base = @import("../../../db/test_fixtures.zig");
const error_registry = @import("../../../errors/error_registry.zig");
const harness_mod = @import("../../test_harness.zig");
const scope_fixtures = @import("../../test_scope_tokens.zig");

const TestHarness = harness_mod.TestHarness;
const ALLOC = std.testing.allocator;

// The workspace the TENANT_ADMIN persona's claims name. `authorizeWorkspace`
// reads the principal, so the grant routes can only be driven under this id.
const TENANT_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f01";
const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";

// A second tenant's workspace, used only as the address the principal must be
// refused at.
const FOREIGN_TENANT_ID = "0196a300-0000-7000-8000-00000000b001";
const FOREIGN_WORKSPACE_ID = "0196a300-0000-7000-8000-00000000b002";

const FLEET_ID = "0196a300-0000-7000-8000-00000000b003";
/// Syntactically valid, names no fleet row.
const FLEET_ID_ABSENT = "0196a300-0000-7000-8000-00000000b0ff";

const GRANT_ID = "0196a300-0000-7000-8000-00000000b004";
const GRANT_ID_ABSENT = "0196a300-0000-7000-8000-00000000b0fe";

const SERVICE = "github";
const STATUS_APPROVED = "approved";
const REASON = "ship the repair pull request";
const FLEET_NAME = "integration-grant-verbs";
const FLEET_CONFIG = "{}";
const FLEET_SOURCE = "# integration grant fixture";

const CREATED_AT_MS: i64 = 1_760_000_000_000;
const APPROVED_AT_MS: i64 = CREATED_AT_MS + 1_000;

const TOKEN = scope_fixtures.TENANT_ADMIN;

fn configureRegistry(_: *auth_mw.MiddlewareRegistry, _: *TestHarness) anyerror!void {}

fn startHarness(alloc: std.mem.Allocator) !*TestHarness {
    return TestHarness.start(alloc, .{
        .configureRegistry = configureRegistry,
        .inline_jwks_json = scope_fixtures.JWKS,
        .issuer = scope_fixtures.ISSUER,
        .audience = scope_fixtures.AUDIENCE,
    });
}

fn grantsPath(alloc: std.mem.Allocator, workspace_id: []const u8, fleet_id: []const u8) ![]const u8 {
    return std.fmt.allocPrint(
        alloc,
        "/v1/workspaces/{s}/fleets/{s}/integration-grants",
        .{ workspace_id, fleet_id },
    );
}

fn revokePath(alloc: std.mem.Allocator, workspace_id: []const u8, fleet_id: []const u8, grant_id: []const u8) ![]const u8 {
    return std.fmt.allocPrint(
        alloc,
        "/v1/workspaces/{s}/fleets/{s}/integration-grants/{s}",
        .{ workspace_id, fleet_id, grant_id },
    );
}

/// Seeds only the fleet and its grant under the persona's existing workspace.
/// The workspace and tenant are shared with sibling suites, so this never tears
/// them down — a suite that removed them would take the neighbours' fixtures
/// with it.
fn seed(h: *TestHarness) !void {
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedTenantById(conn, TENANT_ID, FLEET_NAME);
    try base.seedWorkspaceWithTenant(conn, WORKSPACE_ID, TENANT_ID);
    try base.seedFleet(conn, FLEET_ID, WORKSPACE_ID, FLEET_NAME, FLEET_CONFIG, FLEET_SOURCE);
    _ = try conn.exec(
        \\INSERT INTO core.integration_grants
        \\  (id, fleet_id, service, status, requested_reason, approved_at, created_at)
        \\VALUES ($1::uuid, $2::uuid, $3, $4, $5, $6, $7)
        \\ON CONFLICT (fleet_id, service) DO UPDATE
        \\  SET status = EXCLUDED.status, revoked_at = NULL, approved_at = EXCLUDED.approved_at
    , .{ GRANT_ID, FLEET_ID, SERVICE, STATUS_APPROVED, REASON, APPROVED_AT_MS, CREATED_AT_MS });
}

/// The address the principal is refused at. Its own tenant, so nothing here
/// touches the shared one.
fn seedForeignWorkspace(h: *TestHarness) !void {
    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    try base.seedTenantById(conn, FOREIGN_TENANT_ID, "integration-grant-foreign");
    try base.seedWorkspaceWithTenant(conn, FOREIGN_WORKSPACE_ID, FOREIGN_TENANT_ID);
}

/// Explicit, never deferred at the suite level: deferred cleanup leaks pool
/// connections at `pool.deinit()`.
fn cleanup(h: *TestHarness) void {
    const conn = h.acquireConn() catch return;
    defer h.releaseConn(conn);
    _ = conn.exec("DELETE FROM core.integration_grants WHERE fleet_id = $1::uuid", .{FLEET_ID}) catch |err|
        std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    _ = conn.exec("DELETE FROM core.fleets WHERE id = $1::uuid", .{FLEET_ID}) catch |err|
        std.log.warn("cleanup ignored: {s}", .{@errorName(err)});
    base.teardownWorkspace(conn, FOREIGN_WORKSPACE_ID);
    base.teardownTenantById(conn, FOREIGN_TENANT_ID);
}

test "integration: the grant list reports the fleet's own grants" {
    const h = startHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    try seed(h);

    const path = try grantsPath(ALLOC, WORKSPACE_ID, FLEET_ID);
    defer ALLOC.free(path);
    const r = try (try h.get(path).bearer(TOKEN)).send();
    defer r.deinit();
    try r.expectStatus(.ok);
    // The row's own columns, not merely a non-empty list: the handler copies
    // each field individually, so a projection that shifted by one would still
    // return a plausible list.
    try std.testing.expect(r.bodyContains(SERVICE));
    try std.testing.expect(r.bodyContains(STATUS_APPROVED));
    try std.testing.expect(r.bodyContains(REASON));

    cleanup(h);
}

test "integration: the grant list refuses a workspace the principal does not hold" {
    const h = startHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    try seed(h);
    try seedForeignWorkspace(h);

    // Forbidden rather than not-found: the workspace check runs before the fleet
    // is looked up, so this arm is distinct from the foreign-fleet 404 the IDOR
    // suite already drives.
    const path = try grantsPath(ALLOC, FOREIGN_WORKSPACE_ID, FLEET_ID);
    defer ALLOC.free(path);
    const r = try (try h.get(path).bearer(TOKEN)).send();
    defer r.deinit();
    try r.expectStatus(.forbidden);
    try r.expectErrorCode(error_registry.ERR_FORBIDDEN);

    cleanup(h);
}

test "integration: the grant list answers not-found for a fleet that does not exist" {
    const h = startHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    try seed(h);

    const path = try grantsPath(ALLOC, WORKSPACE_ID, FLEET_ID_ABSENT);
    defer ALLOC.free(path);
    const r = try (try h.get(path).bearer(TOKEN)).send();
    defer r.deinit();
    try r.expectStatus(.not_found);
    try r.expectErrorCode(error_registry.ERR_AGENTSFLEET_NOT_FOUND);

    cleanup(h);
}

test "integration: revoking a grant stands it down once and refuses the second attempt" {
    const h = startHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    try seed(h);

    const path = try revokePath(ALLOC, WORKSPACE_ID, FLEET_ID, GRANT_ID);
    defer ALLOC.free(path);
    {
        const r = try (try h.delete(path).bearer(TOKEN)).send();
        defer r.deinit();
        try r.expectStatus(.no_content);
    }
    // The statement matches on `status != revoked`, so the retry finds no row.
    // A handler that reported success here would tell the dashboard it had just
    // revoked something it had not touched.
    {
        const r = try (try h.delete(path).bearer(TOKEN)).send();
        defer r.deinit();
        try r.expectStatus(.not_found);
        try r.expectErrorCode(error_registry.ERR_GRANT_REVOKE_NOT_FOUND);
    }
    // And the list now reports it revoked rather than dropping it.
    {
        const list_path = try grantsPath(ALLOC, WORKSPACE_ID, FLEET_ID);
        defer ALLOC.free(list_path);
        const r = try (try h.get(list_path).bearer(TOKEN)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains("revoked"));
    }

    cleanup(h);
}

test "integration: revoke refuses a foreign workspace and an absent fleet distinctly" {
    const h = startHarness(ALLOC) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();
    try seed(h);
    try seedForeignWorkspace(h);

    {
        const path = try revokePath(ALLOC, FOREIGN_WORKSPACE_ID, FLEET_ID, GRANT_ID);
        defer ALLOC.free(path);
        const r = try (try h.delete(path).bearer(TOKEN)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
        try r.expectErrorCode(error_registry.ERR_FORBIDDEN);
    }
    {
        const path = try revokePath(ALLOC, WORKSPACE_ID, FLEET_ID_ABSENT, GRANT_ID);
        defer ALLOC.free(path);
        const r = try (try h.delete(path).bearer(TOKEN)).send();
        defer r.deinit();
        try r.expectStatus(.not_found);
        try r.expectErrorCode(error_registry.ERR_AGENTSFLEET_NOT_FOUND);
    }
    // A grant id that names nothing, on a fleet that does exist: the fleet
    // checks pass and the statement itself is what refuses.
    {
        const path = try revokePath(ALLOC, WORKSPACE_ID, FLEET_ID, GRANT_ID_ABSENT);
        defer ALLOC.free(path);
        const r = try (try h.delete(path).bearer(TOKEN)).send();
        defer r.deinit();
        try r.expectStatus(.not_found);
        try r.expectErrorCode(error_registry.ERR_GRANT_REVOKE_NOT_FOUND);
    }

    cleanup(h);
}
