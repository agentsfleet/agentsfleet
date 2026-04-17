/// test_fixtures_prompt_events.zig — fixtures for prompt_lifecycle_events.
///
/// Covers: src/observability/prompt_events.zig integration tests against the
/// real `core.prompt_lifecycle_events` table (schema/003).
///
/// The production schema attaches BEFORE UPDATE + BEFORE DELETE triggers
/// that raise on any mutation (append-only invariant). That forbids normal
/// DELETE during teardown — including the cascade fired by deleting the
/// parent workspace. The cleanup path therefore temporarily switches the
/// session into `replica` replication role, which suspends non-`ALWAYS`
/// triggers, performs the DELETEs, then switches back to `origin`.
///
/// Superuser-only — matches the docker-compose dev DB. If your test DB
/// runs as a non-superuser role, tests using this fixture will skip on the
/// `SET session_replication_role` step rather than mutating rows.

const base = @import("test_fixtures.zig");
const pg = @import("pg");

// Segment 5 `bb00...` isolates this fixture's IDs from uc1/uc3 (aa/bb/cc
// ranges elsewhere in this repo's test fixtures).
pub const TENANT_ID = "0195b4ba-8d3a-7f13-8abc-bb0000000001";
pub const WORKSPACE_ID = "0195b4ba-8d3a-7f13-8abc-bb0000000011";

/// Deterministic agent / config-version UUIDs used by the append-only test.
/// Must be UUIDv7 (CHECK constraint on the table isn't on these columns, but
/// staying v7-compatible keeps the values recognisable in logs).
pub const AGENT_ID = "0195b4ba-8d3a-7f13-8abc-bb0000000021";
pub const CONFIG_VERSION_ID = "0195b4ba-8d3a-7f13-8abc-bb0000000031";

pub fn seed(conn: *pg.Conn) !void {
    try base.seedTenantById(conn, TENANT_ID, "prompt-events-tests");
    try base.seedWorkspaceWithTenant(conn, WORKSPACE_ID, TENANT_ID);
}

/// Teardown order: events → workspace → tenant. The append-only triggers on
/// `core.prompt_lifecycle_events` block plain DELETE; suspend them via
/// `session_replication_role = replica` for the duration of cleanup, then
/// restore. Idempotent; safe as both a pre-seed reset and post-test defer.
pub fn cleanup(conn: *pg.Conn) void {
    _ = conn.exec("SET session_replication_role = 'replica'", .{}) catch {};
    _ = conn.exec(
        "DELETE FROM core.prompt_lifecycle_events WHERE workspace_id = $1::uuid",
        .{WORKSPACE_ID},
    ) catch {};
    _ = conn.exec(
        "DELETE FROM core.workspaces WHERE workspace_id = $1::uuid",
        .{WORKSPACE_ID},
    ) catch {};
    _ = conn.exec(
        "DELETE FROM core.tenants WHERE tenant_id = $1::uuid",
        .{TENANT_ID},
    ) catch {};
    _ = conn.exec("SET session_replication_role = 'origin'", .{}) catch {};
}
