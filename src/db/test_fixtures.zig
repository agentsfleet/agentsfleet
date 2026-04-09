/// test_fixtures.zig — shared base helpers for all DB integration tests.
///
/// Usage pattern (every integration test):
///
///   const db_ctx = (try base.openTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
///   defer db_ctx.pool.deinit();
///   defer db_ctx.pool.release(db_ctx.conn);
///
///   try base.seedTenant(db_ctx.conn);
///   defer base.teardownTenant(db_ctx.conn);
///   try base.seedWorkspace(db_ctx.conn, workspace_id);
///   defer base.teardownWorkspace(db_ctx.conn, workspace_id);
///
/// Teardown order matters — workspace must be deleted before tenant
/// (FK: workspaces.tenant_id → tenants.tenant_id, NO ACTION).
/// Deleting the workspace cascades most child tables automatically; see the
/// FK cascade map in docs/spec for the full list.
///
/// All seed inserts use ON CONFLICT DO NOTHING — safe to call multiple times
/// even if a prior test run panicked before teardown.
const std = @import("std");
const pg = @import("pg");
const db = @import("pool.zig");

/// Canonical test tenant shared across all integration test UCs.
pub const TEST_TENANT_ID = "0195b4ba-8d3a-7f13-8abc-000000000001";

/// Insert the canonical test tenant. Idempotent via ON CONFLICT DO NOTHING.
pub fn seedTenant(conn: *pg.Conn) !void {
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, api_key_hash, created_at, updated_at)
        \\VALUES ($1, 'scrooge-mcduck', 'testhash-placeholder', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{TEST_TENANT_ID});
}

/// Insert a test workspace. Requires seedTenant to have run first.
/// Idempotent via ON CONFLICT DO NOTHING.
pub fn seedWorkspace(conn: *pg.Conn, workspace_id: []const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO workspaces
        \\  (workspace_id, tenant_id, repo_url, default_branch, created_at, updated_at)
        \\VALUES ($1, $2, 'https://github.com/scrooge-mcduck/duckburg-monorepo.git', 'main', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ workspace_id, TEST_TENANT_ID });
}

/// Delete workspace. CASCADE removes:
///   workspace_credit_state, workspace_credit_audit, workspace_billing_state,
///   workspace_billing_audit, usage_ledger, workspace_entitlements,
///   workspace_active_config, entitlement_policy_audit_snapshots,
///   agent_improvement_proposals, harness_change_log,
///   agent_run_analysis, agent_run_scores.
///
/// Does NOT cascade: agent_profiles — but MUST be called before deleting agent_profiles
/// because workspace_active_config holds a FK to agent_config_versions (NO ACTION).
/// Correct order: teardownWorkspace → delete agent_profiles → teardownTenant.
pub fn teardownWorkspace(conn: *pg.Conn, workspace_id: []const u8) void {
    _ = conn.exec(
        "DELETE FROM workspaces WHERE workspace_id = $1::uuid",
        .{workspace_id},
    ) catch {};
}

/// Delete the canonical test tenant.
/// Call only after all workspaces for this tenant have been removed.
pub fn teardownTenant(conn: *pg.Conn) void {
    teardownTenantById(conn, TEST_TENANT_ID);
}

// ── Multi-tenant helpers ────────────────────────────────────────────────

/// Insert a tenant with a custom ID. Idempotent via ON CONFLICT DO NOTHING.
pub fn seedTenantById(conn: *pg.Conn, tenant_id: []const u8, name: []const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO tenants (tenant_id, name, api_key_hash, created_at, updated_at)
        \\VALUES ($1, $2, 'testhash-placeholder', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ tenant_id, name });
}

/// Insert a workspace under a specific tenant. Idempotent.
pub fn seedWorkspaceWithTenant(conn: *pg.Conn, workspace_id: []const u8, tenant_id: []const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO workspaces
        \\  (workspace_id, tenant_id, repo_url, default_branch, created_at, updated_at)
        \\VALUES ($1, $2, 'https://github.com/scrooge-mcduck/duckburg-monorepo.git', 'main', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ workspace_id, tenant_id });
}

/// Delete a tenant by custom ID.
pub fn teardownTenantById(conn: *pg.Conn, tenant_id: []const u8) void {
    _ = conn.exec(
        "DELETE FROM tenants WHERE tenant_id = $1::uuid",
        .{tenant_id},
    ) catch {};
}

// ── Run/spec helpers (for tests that touch usage_ledger or billing_delivery_outbox) ──

/// Insert a minimal spec row. Workspace must exist. Idempotent.
pub fn seedSpec(conn: *pg.Conn, spec_id: []const u8, workspace_id: []const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO specs (spec_id, workspace_id, tenant_id, file_path, title, status, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, 'Test Spec', 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ spec_id, workspace_id, TEST_TENANT_ID, spec_id });
}

/// Insert a minimal run row. Spec and workspace must exist. Idempotent.
/// Uses run_id as idempotency_key for uniqueness.
pub fn seedRun(conn: *pg.Conn, run_id: []const u8, workspace_id: []const u8, spec_id: []const u8) !void {
    _ = try conn.exec(
        \\INSERT INTO runs
        \\  (run_id, workspace_id, spec_id, tenant_id, state, attempt, mode,
        \\   requested_by, idempotency_key, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, 'completed', 1, 'auto', 'test', $5, 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ run_id, workspace_id, spec_id, TEST_TENANT_ID, run_id });
}

/// Delete runs for a workspace. Call before teardownWorkspace if runs were seeded,
/// since runs has FK to workspaces without CASCADE.
pub fn teardownRuns(conn: *pg.Conn, workspace_id: []const u8) void {
    _ = conn.exec(
        "DELETE FROM runs WHERE workspace_id = $1::uuid",
        .{workspace_id},
    ) catch {};
}

/// Delete specs for a workspace. Call after teardownRuns (runs FK → specs).
pub fn teardownSpecs(conn: *pg.Conn, workspace_id: []const u8) void {
    _ = conn.exec(
        "DELETE FROM specs WHERE workspace_id = $1::uuid",
        .{workspace_id},
    ) catch {};
}

// ── Zombie helpers (M1_001 — event loop integration tests) ─────────────

/// Insert a minimal zombie row. Workspace must exist. Idempotent.
pub fn seedZombie(
    conn: *pg.Conn,
    zombie_id: []const u8,
    workspace_id: []const u8,
    name: []const u8,
    config_json: []const u8,
    source_markdown: []const u8,
) !void {
    _ = try conn.exec(
        \\INSERT INTO core.zombies
        \\  (id, workspace_id, name, source_markdown, config_json, status, created_at, updated_at)
        \\VALUES ($1, $2, $3, $4, $5, 'active', 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ zombie_id, workspace_id, name, source_markdown, config_json });
}

/// Insert a zombie session checkpoint. Zombie must exist. Idempotent.
pub fn seedZombieSession(
    conn: *pg.Conn,
    session_id: []const u8,
    zombie_id: []const u8,
    context_json: []const u8,
) !void {
    _ = try conn.exec(
        \\INSERT INTO core.zombie_sessions
        \\  (id, zombie_id, context_json, checkpoint_at, created_at, updated_at)
        \\VALUES ($1, $2, $3, 0, 0, 0)
        \\ON CONFLICT (zombie_id) DO UPDATE
        \\  SET context_json = EXCLUDED.context_json
    , .{ session_id, zombie_id, context_json });
}

/// Delete zombies for a workspace. Cascades to zombie_sessions (FK).
pub fn teardownZombies(conn: *pg.Conn, workspace_id: []const u8) void {
    // Sessions first (FK to zombies), then zombies.
    _ = conn.exec(
        \\DELETE FROM core.zombie_sessions WHERE zombie_id IN
        \\  (SELECT id FROM core.zombies WHERE workspace_id = $1)
    , .{workspace_id}) catch {};
    _ = conn.exec(
        \\DELETE FROM core.activity_events WHERE workspace_id = $1
    , .{workspace_id}) catch {};
    _ = conn.exec(
        "DELETE FROM core.zombies WHERE workspace_id = $1",
        .{workspace_id},
    ) catch {};
}

// ── Shared DB connection ────────────────────────────────────────────────

/// Open a test DB connection. Returns null when TEST_DATABASE_URL / DATABASE_URL
/// is unset, causing the test to be skipped via `return error.SkipZigTest`.
///
/// Uses page_allocator for URL parse results so they outlive the pool. pg.Pool
/// stores shallow references to host/auth strings — if parsed via an arena that
/// is freed first, pool.release() crashes on non-idle connections.
pub fn openTestConn(alloc: std.mem.Allocator) !?struct { pool: *pg.Pool, conn: *pg.Conn } {
    const url = std.process.getEnvVarOwned(alloc, "TEST_DATABASE_URL") catch
        std.process.getEnvVarOwned(alloc, "DATABASE_URL") catch return null;
    defer alloc.free(url);

    const opts = try db.parseUrl(std.heap.page_allocator, url);
    const pool = try pg.Pool.init(alloc, opts);
    errdefer pool.deinit();
    const conn = try pool.acquire();
    return .{ .pool = pool, .conn = conn };
}
