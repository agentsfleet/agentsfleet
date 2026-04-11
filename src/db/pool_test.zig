const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("pg_query.zig").PgQuery;
const id_format = @import("../types/id_format.zig");
const pool_mod = @import("pool.zig");

const Pool = pool_mod.Pool;
const Conn = pool_mod.Conn;
const parseUrl = pool_mod.parseUrl;
const roleEnvVarName = pool_mod.roleEnvVarName;

test "parseUrl parses host, port, db, credentials" {
    const alloc = std.testing.allocator;
    const opts = try parseUrl(alloc, "postgres://alice:secret@localhost:5433/usezombiedb");
    defer alloc.free(opts.connect.host.?);
    defer alloc.free(opts.auth.username);
    const password = opts.auth.password.?;
    defer alloc.free(password);
    const database = opts.auth.database.?;
    defer alloc.free(database);

    try std.testing.expectEqualStrings("localhost", opts.connect.host.?);
    try std.testing.expectEqual(@as(u16, 5433), opts.connect.port.?);
    try std.testing.expectEqualStrings("alice", opts.auth.username);
    try std.testing.expectEqualStrings("secret", password);
    try std.testing.expectEqualStrings("usezombiedb", database);
}

test "parseUrl sets tls=require on standard URL" {
    const alloc = std.testing.allocator;
    const opts = try parseUrl(alloc, "postgresql://api_user:pw@db.example.com:5432/mydb");
    defer alloc.free(opts.connect.host.?);
    defer alloc.free(opts.auth.username);
    defer alloc.free(opts.auth.password.?);
    defer alloc.free(opts.auth.database.?);

    try std.testing.expectEqualStrings("mydb", opts.auth.database.?);
    try std.testing.expect(opts.connect.tls == .require);
}

test "parseUrl strips query string from dbname" {
    const alloc = std.testing.allocator;
    const opts = try parseUrl(alloc, "postgres://u:p@host:5432/zombiedb?sslmode=require");
    defer alloc.free(opts.connect.host.?);
    defer alloc.free(opts.auth.username);
    defer alloc.free(opts.auth.password.?);
    defer alloc.free(opts.auth.database.?);

    try std.testing.expectEqualStrings("zombiedb", opts.auth.database.?);
    try std.testing.expect(opts.connect.tls == .require);
}

test "parseUrl strips multiple query params from dbname" {
    const alloc = std.testing.allocator;
    const opts = try parseUrl(alloc, "postgres://u:p@host:5432/mydb?sslmode=require&application_name=worker");
    defer alloc.free(opts.connect.host.?);
    defer alloc.free(opts.auth.username);
    defer alloc.free(opts.auth.password.?);
    defer alloc.free(opts.auth.database.?);

    try std.testing.expectEqualStrings("mydb", opts.auth.database.?);
    try std.testing.expect(opts.connect.tls == .require);
}

test "parseUrl respects sslmode=disable for local dev" {
    const alloc = std.testing.allocator;
    const opts = try parseUrl(alloc, "postgres://u:p@localhost:5432/testdb?sslmode=disable");
    defer alloc.free(opts.connect.host.?);
    defer alloc.free(opts.auth.username);
    defer alloc.free(opts.auth.password.?);
    defer alloc.free(opts.auth.database.?);

    try std.testing.expectEqualStrings("testdb", opts.auth.database.?);
    try std.testing.expect(opts.connect.tls == .off);
}

test "roleEnvVarName maps db roles deterministically" {
    try std.testing.expectEqualStrings("DATABASE_URL", roleEnvVarName(.default));
    try std.testing.expectEqualStrings("DATABASE_URL_API", roleEnvVarName(.api));
    try std.testing.expectEqualStrings("DATABASE_URL_WORKER", roleEnvVarName(.worker));
    try std.testing.expectEqualStrings("DATABASE_URL_CALLBACK", roleEnvVarName(.callback));
    try std.testing.expectEqualStrings("DATABASE_URL_MIGRATOR", roleEnvVarName(.migrator));
}

fn openIntegrationTestConn(alloc: std.mem.Allocator) !?struct { pool: *Pool, conn: *Conn } {
    // DB-backed integration tests must be opt-in via TEST_DATABASE_URL.
    // This avoids accidentally running against unrelated DATABASE_URL values
    // in non-DB test lanes (e.g. CI's _test-integration-zombied target).
    const url = std.process.getEnvVarOwned(alloc, "TEST_DATABASE_URL") catch return null;
    defer alloc.free(url);

    // parseUrl allocates host/auth strings that must outlive the pool.
    // Use page_allocator to keep them process-lifetime, matching production.
    const opts = try parseUrl(std.heap.page_allocator, url);
    const pool = pg.Pool.init(alloc, opts) catch return null;
    errdefer pool.deinit();
    const conn = pool.acquire() catch {
        pool.deinit();
        return null;
    };
    return .{ .pool = pool, .conn = conn };
}

fn createUuidContractTempSchema(conn: *Conn) !void {
    _ = try conn.exec(
        \\CREATE TEMP TABLE t_uuid_runs (
        \\  run_id UUID PRIMARY KEY,
        \\  run_snapshot_version UUID
        \\)
    , .{});
    _ = try conn.exec(
        \\CREATE TEMP TABLE t_uuid_agent_profile_versions (
        \\  profile_version_id UUID PRIMARY KEY
        \\)
    , .{});
    _ = try conn.exec(
        \\CREATE TEMP TABLE t_uuid_profile_compile_jobs (
        \\  compile_job_id UUID PRIMARY KEY
        \\)
    , .{});
    _ = try conn.exec(
        \\CREATE TEMP TABLE t_uuid_run_transitions (
        \\  id UUID PRIMARY KEY,
        \\  run_id UUID NOT NULL REFERENCES t_uuid_runs(run_id)
        \\)
    , .{});
    _ = try conn.exec(
        \\CREATE TEMP TABLE t_uuid_artifacts (
        \\  run_id UUID NOT NULL REFERENCES t_uuid_runs(run_id)
        \\)
    , .{});
    _ = try conn.exec(
        \\CREATE TEMP TABLE t_uuid_usage_ledger (
        \\  run_id UUID NOT NULL REFERENCES t_uuid_runs(run_id)
        \\)
    , .{});
    _ = try conn.exec(
        \\CREATE TEMP TABLE t_uuid_run_side_effects (
        \\  run_id UUID NOT NULL REFERENCES t_uuid_runs(run_id)
        \\)
    , .{});
    _ = try conn.exec(
        \\CREATE TEMP TABLE t_uuid_run_side_effect_outbox (
        \\  run_id UUID NOT NULL REFERENCES t_uuid_runs(run_id)
        \\)
    , .{});
    _ = try conn.exec(
        \\CREATE TEMP TABLE t_uuid_workspace_memories (
        \\  run_id UUID NOT NULL REFERENCES t_uuid_runs(run_id)
        \\)
    , .{});
    _ = try conn.exec(
        \\CREATE TEMP TABLE t_uuid_policy_events (
        \\  run_id UUID REFERENCES t_uuid_runs(run_id)
        \\)
    , .{});
    _ = try conn.exec(
        \\CREATE TEMP TABLE t_uuid_workspace_active_profile (
        \\  profile_version_id UUID NOT NULL REFERENCES t_uuid_agent_profile_versions(profile_version_id)
        \\)
    , .{});
    _ = try conn.exec(
        \\CREATE TEMP TABLE t_uuid_profile_linkage_audit_artifacts (
        \\  profile_version_id UUID NOT NULL REFERENCES t_uuid_agent_profile_versions(profile_version_id),
        \\  compile_job_id UUID REFERENCES t_uuid_profile_compile_jobs(compile_job_id),
        \\  run_id UUID REFERENCES t_uuid_runs(run_id)
        \\)
    , .{});
}

test "integration: canary pool acquire + exec + query SELECT 1" {
    const alloc = std.testing.allocator;
    const url = std.process.getEnvVarOwned(alloc, "TEST_DATABASE_URL") catch
        std.process.getEnvVarOwned(alloc, "DATABASE_URL") catch return error.SkipZigTest;
    defer alloc.free(url);

    const opts = try parseUrl(std.heap.page_allocator, url);
    const pool = try pg.Pool.init(alloc, opts);
    defer pool.deinit();

    const conn = try pool.acquire();
    defer pool.release(conn);

    // Simple query protocol
    _ = try conn.exec("SELECT 1", .{});
    // Extended query protocol (no params)
    _ = try conn.exec("SELECT 1", .{});
}

test "integration: uuid contract tables are UUID typed for run/profile/linkage IDs" {
    if (!std.process.hasEnvVarConstant("LIVE_DB")) return error.SkipZigTest;
    const db_ctx = (try openIntegrationTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createUuidContractTempSchema(db_ctx.conn);

    {
        var q = PgQuery.from(try db_ctx.conn.query(
            \\SELECT table_name, column_name, data_type
            \\FROM information_schema.columns
            \\WHERE table_name IN ('t_uuid_runs', 't_uuid_agent_profile_versions', 't_uuid_profile_compile_jobs', 't_uuid_profile_linkage_audit_artifacts')
            \\  AND column_name IN ('run_id', 'run_snapshot_version', 'profile_version_id', 'compile_job_id')
            \\ORDER BY table_name, column_name
        , .{}));
        defer q.deinit();

        while (try q.next()) |row| {
            try std.testing.expectEqualStrings("uuid", try row.get([]const u8, 2));
        }
    }

    const run_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f99";
    const pver_id = "0195b4ba-8d3a-7f13-9abc-2b3e1e0a6f98";
    const cjob_id = "0195b4ba-8d3a-7f13-aabc-2b3e1e0a6f97";
    _ = try db_ctx.conn.exec(
        "INSERT INTO t_uuid_runs (run_id, run_snapshot_version) VALUES ($1::uuid, $2::uuid)",
        .{ run_id, pver_id },
    );
    _ = try db_ctx.conn.exec(
        "INSERT INTO t_uuid_agent_profile_versions (profile_version_id) VALUES ($1::uuid)",
        .{pver_id},
    );
    _ = try db_ctx.conn.exec(
        "INSERT INTO t_uuid_profile_compile_jobs (compile_job_id) VALUES ($1::uuid)",
        .{cjob_id},
    );
    _ = try db_ctx.conn.exec(
        "INSERT INTO t_uuid_profile_linkage_audit_artifacts (profile_version_id, compile_job_id, run_id) VALUES ($1::uuid, $2::uuid, $3::uuid)",
        .{ pver_id, cjob_id, run_id },
    );
}

test "T6 integration: generated UUID PKs round-trip through INSERT and SELECT" {
    if (!std.process.hasEnvVarConstant("LIVE_DB")) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const db_ctx = (try openIntegrationTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    // Create TEMP tables with UUID PK + CHECK constraint (mirrors real schema)
    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE t6_run_transitions (
        \\  id UUID PRIMARY KEY,
        \\  CONSTRAINT ck_t6_rt_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
        \\  run_id TEXT NOT NULL,
        \\  ts BIGINT NOT NULL
        \\)
    , .{});
    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE t6_usage_ledger (
        \\  id UUID PRIMARY KEY,
        \\  CONSTRAINT ck_t6_ul_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
        \\  run_id TEXT NOT NULL
        \\)
    , .{});
    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE t6_policy_events (
        \\  id UUID PRIMARY KEY,
        \\  CONSTRAINT ck_t6_pe_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
        \\  workspace_id TEXT NOT NULL
        \\)
    , .{});

    // INSERT with generated ids
    const tid = try id_format.generateTransitionId(alloc);
    defer alloc.free(tid);
    const uid = try id_format.generateUsageLedgerId(alloc);
    defer alloc.free(uid);
    const pid = try id_format.generatePolicyEventId(alloc);
    defer alloc.free(pid);

    _ = try db_ctx.conn.exec(
        "INSERT INTO t6_run_transitions (id, run_id, ts) VALUES ($1::uuid, 'run-1', 1000)",
        .{tid},
    );
    _ = try db_ctx.conn.exec(
        "INSERT INTO t6_usage_ledger (id, run_id) VALUES ($1::uuid, 'run-1')",
        .{uid},
    );
    _ = try db_ctx.conn.exec(
        "INSERT INTO t6_policy_events (id, workspace_id) VALUES ($1::uuid, 'ws-1')",
        .{pid},
    );

    // SELECT and verify round-trip: id::text matches original string
    {
        var q = PgQuery.from(try db_ctx.conn.query(
            "SELECT id::text FROM t6_run_transitions WHERE id = $1::uuid",
            .{tid},
        ));
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqualStrings(tid, try row.get([]const u8, 0));
    }
    {
        var q = PgQuery.from(try db_ctx.conn.query(
            "SELECT id::text FROM t6_usage_ledger WHERE id = $1::uuid",
            .{uid},
        ));
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqualStrings(uid, try row.get([]const u8, 0));
    }
    {
        var q = PgQuery.from(try db_ctx.conn.query(
            "SELECT id::text FROM t6_policy_events WHERE id = $1::uuid",
            .{pid},
        ));
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqualStrings(pid, try row.get([]const u8, 0));
    }
}

test "T6 integration: UUID CHECK constraint rejects non-v7 ids" {
    if (!std.process.hasEnvVarConstant("LIVE_DB")) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const db_ctx = (try openIntegrationTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE t6_check_reject (
        \\  id UUID PRIMARY KEY,
        \\  CONSTRAINT ck_t6_cr_uuidv7 CHECK (substring(id::text from 15 for 1) = '7')
        \\)
    , .{});

    // v4 UUID must be rejected by the CHECK constraint
    try std.testing.expectError(error.PG, db_ctx.conn.exec(
        "INSERT INTO t6_check_reject (id) VALUES ('a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11'::uuid)",
        .{},
    ));
}

test "T6 integration: duplicate UUID PK is rejected" {
    if (!std.process.hasEnvVarConstant("LIVE_DB")) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const db_ctx = (try openIntegrationTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    _ = try db_ctx.conn.exec(
        \\CREATE TEMP TABLE t6_dup_reject (
        \\  id UUID PRIMARY KEY,
        \\  CONSTRAINT ck_t6_dr_uuidv7 CHECK (substring(id::text from 15 for 1) = '7')
        \\)
    , .{});

    const dup_id = try id_format.generateTransitionId(alloc);
    defer alloc.free(dup_id);

    _ = try db_ctx.conn.exec(
        "INSERT INTO t6_dup_reject (id) VALUES ($1::uuid)",
        .{dup_id},
    );
    // Second insert with same id must fail (PK violation)
    try std.testing.expectError(error.PG, db_ctx.conn.exec(
        "INSERT INTO t6_dup_reject (id) VALUES ($1::uuid)",
        .{dup_id},
    ));
}

test "integration: audit schema exists and contains migration bookkeeping tables" {
    if (!std.process.hasEnvVarConstant("LIVE_DB")) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const db_ctx = (try openIntegrationTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    // Verify audit schema exists
    {
        var q = PgQuery.from(try db_ctx.conn.query(
            "SELECT 1 FROM information_schema.schemata WHERE schema_name = 'audit'",
            .{},
        ));
        defer q.deinit();
        const row = try q.next();
        try std.testing.expect(row != null);
    }

    // Verify audit.schema_migrations exists and is queryable
    {
        var q = PgQuery.from(try db_ctx.conn.query(
            "SELECT COUNT(*) FROM audit.schema_migrations",
            .{},
        ));
        defer q.deinit();
        const row = (try q.next()) orelse return error.SkipZigTest;
        const count = try row.get(i64, 0);
        try std.testing.expect(count > 0);
    }

    // Verify audit.schema_migration_failures exists and is queryable
    {
        var q = PgQuery.from(try db_ctx.conn.query(
            "SELECT COUNT(*) FROM audit.schema_migration_failures",
            .{},
        ));
        defer q.deinit();
        _ = try q.next();
    }

    // Verify schema_migrations is NOT in public schema
    {
        var q = PgQuery.from(try db_ctx.conn.query(
            \\SELECT 1 FROM information_schema.tables
            \\WHERE table_schema = 'public' AND table_name = 'schema_migrations'
        , .{}));
        defer q.deinit();
        const row = try q.next();
        try std.testing.expect(row == null);
    }
}

test "integration: db_migrator role exists after migration" {
    if (!std.process.hasEnvVarConstant("LIVE_DB")) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const db_ctx = (try openIntegrationTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    var q = PgQuery.from(try db_ctx.conn.query(
        "SELECT 1 FROM pg_roles WHERE rolname = 'db_migrator'",
        .{},
    ));
    defer q.deinit();
    const row = try q.next();
    try std.testing.expect(row != null);
}

test "integration: zero-trust schema segmentation and role matrix are enforced" {
    if (!std.process.hasEnvVarConstant("LIVE_DB")) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const db_ctx = (try openIntegrationTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    const schema_checks = [_][]const u8{ "core", "agent", "billing", "vault", "audit", "ops_ro" };
    inline for (schema_checks) |schema_name| {
        var schema_q = PgQuery.from(try db_ctx.conn.query(
            "SELECT 1 FROM information_schema.schemata WHERE schema_name = $1",
            .{schema_name},
        ));
        defer schema_q.deinit();
        try std.testing.expect((try schema_q.next()) != null);
    }

    // public should not own authoritative app tables.
    {
        var q = PgQuery.from(try db_ctx.conn.query(
            \\SELECT 1
            \\FROM information_schema.tables
            \\WHERE table_schema = 'public'
            \\  AND table_name IN ('tenants', 'workspaces', 'runs', 'agent_profiles', 'workspace_entitlements')
            \\LIMIT 1
        , .{}));
        defer q.deinit();
        try std.testing.expect((try q.next()) == null);
    }

    const role_checks = [_][]const u8{
        "db_migrator",
        "api_runtime",
        "worker_runtime",
        "ops_readonly_human",
        "ops_readonly_agent",
    };
    inline for (role_checks) |role_name| {
        var role_q = PgQuery.from(try db_ctx.conn.query(
            "SELECT 1 FROM pg_roles WHERE rolname = $1",
            .{role_name},
        ));
        defer role_q.deinit();
        try std.testing.expect((try role_q.next()) != null);
    }

    const worker_privilege_checks = [_]struct { table_name: []const u8, privilege: []const u8 }{
        .{ .table_name = "agent.agent_profiles", .privilege = "UPDATE" },
        .{ .table_name = "agent.workspace_active_config", .privilege = "UPDATE" },
        .{ .table_name = "agent.agent_config_versions", .privilege = "INSERT" },
        // M10_001: agent.harness_change_log removed (pipeline v1 scoring).
    };
    inline for (worker_privilege_checks) |check| {
        var privilege_q = PgQuery.from(try db_ctx.conn.query(
            "SELECT has_table_privilege('worker_runtime', $1, $2)",
            .{ check.table_name, check.privilege },
        ));
        defer privilege_q.deinit();
        const row = (try privilege_q.next()) orelse return error.TestUnexpectedResult;
        const has_privilege = try row.get(bool, 0);
        try std.testing.expect(has_privilege);
    }

    const rls_tables = [_]struct { schema_name: []const u8, table_name: []const u8 }{
        .{ .schema_name = "agent", .table_name = "agent_profiles" },
        .{ .schema_name = "agent", .table_name = "agent_config_versions" },
        .{ .schema_name = "agent", .table_name = "workspace_active_config" },
        .{ .schema_name = "agent", .table_name = "config_compile_jobs" },
        .{ .schema_name = "agent", .table_name = "config_linkage_audit_artifacts" },
        .{ .schema_name = "vault", .table_name = "workspace_skill_secrets" },
    };
    inline for (rls_tables) |table_ref| {
        var rls_q = PgQuery.from(try db_ctx.conn.query(
            \\SELECT c.relrowsecurity
            \\FROM pg_class c
            \\JOIN pg_namespace n ON n.oid = c.relnamespace
            \\WHERE n.nspname = $1
            \\  AND c.relname = $2
        , .{ table_ref.schema_name, table_ref.table_name }));
        defer rls_q.deinit();
        const row = (try rls_q.next()) orelse return error.TestUnexpectedResult;
        const rls_enabled = try row.get(bool, 0);
        try std.testing.expect(rls_enabled);
    }
}

test "integration: runMigrations is idempotent when table exists but migration record is absent" {
    if (!std.process.hasEnvVarConstant("LIVE_DB")) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const db_ctx = (try openIntegrationTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    // Version well above all real migrations to avoid collisions.
    const test_version: i32 = 99998;
    const test_sql =
        \\CREATE TABLE IF NOT EXISTS public.test_migration_idempotency_fixture (id BIGINT PRIMARY KEY);
    ;
    const test_migrations = [_]pool_mod.Migration{
        .{ .version = test_version, .sql = test_sql },
    };

    // Clean slate from any previous interrupted test run.
    _ = db_ctx.conn.exec("DELETE FROM audit.schema_migrations WHERE version = $1", .{test_version}) catch {};
    _ = db_ctx.conn.exec("DROP TABLE IF EXISTS public.test_migration_idempotency_fixture", .{}) catch {};

    // First run: applies normally, table and record created.
    try pool_mod.runMigrations(db_ctx.pool, &test_migrations);

    // Verify table exists.
    {
        var q = PgQuery.from(try db_ctx.conn.query(
            "SELECT 1 FROM information_schema.tables WHERE table_schema='public' AND table_name='test_migration_idempotency_fixture'",
            .{},
        ));
        defer q.deinit();
        try std.testing.expect((try q.next()) != null);
    }

    // Simulate state inconsistency: drop the migration record, leave the table.
    _ = try db_ctx.conn.exec("DELETE FROM audit.schema_migrations WHERE version = $1", .{test_version});

    // Second run: table exists, record absent. Must succeed and re-insert the record.
    try pool_mod.runMigrations(db_ctx.pool, &test_migrations);

    // Verify the migration record was re-inserted.
    {
        var q = PgQuery.from(try db_ctx.conn.query(
            "SELECT 1 FROM audit.schema_migrations WHERE version = $1",
            .{test_version},
        ));
        defer q.deinit();
        try std.testing.expect((try q.next()) != null);
    }

    // Cleanup.
    _ = db_ctx.conn.exec("DELETE FROM audit.schema_migrations WHERE version = $1", .{test_version}) catch {};
    _ = db_ctx.conn.exec("DROP TABLE IF EXISTS public.test_migration_idempotency_fixture", .{}) catch {};
}

test "integration: readonly roles can only query ops_ro views, not vault" {
    if (!std.process.hasEnvVarConstant("LIVE_DB")) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const db_ctx = (try openIntegrationTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    {
        var q = PgQuery.from(try db_ctx.conn.query(
            "SELECT has_table_privilege('ops_readonly_agent', 'vault.secrets', 'SELECT')",
            .{},
        ));
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        const can_read_vault = try row.get(bool, 0);
        try std.testing.expect(!can_read_vault);
    }

    {
        var q = PgQuery.from(try db_ctx.conn.query(
            "SELECT has_table_privilege('ops_readonly_agent', 'ops_ro.workspace_overview', 'SELECT')",
            .{},
        ));
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        const can_read_view = try row.get(bool, 0);
        try std.testing.expect(can_read_view);
    }

    {
        var q = PgQuery.from(try db_ctx.conn.query(
            "SELECT has_table_privilege('ops_readonly_human', 'ops_ro.billing_overview', 'SELECT')",
            .{},
        ));
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        const can_read_view = try row.get(bool, 0);
        try std.testing.expect(can_read_view);
    }
}
