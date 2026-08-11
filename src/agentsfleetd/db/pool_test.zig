const std = @import("std");
const clock = @import("common").clock;
const env = @import("common").env;
const pg = @import("pg");
const PgQuery = @import("pg_query.zig").PgQuery;
const id_format = @import("../types/id_format.zig");
const pool_mod = @import("pool.zig");
const test_fixtures = @import("test_fixtures.zig");
const migration_lock = @import("pool_migration_lock.zig");
/// runMigrations prunes audit.schema_migrations rows not in ITS array — so a
/// test that feeds it a fixture subset wipes the canonical bookkeeping rows,
/// and any later-ordered test asserting on audit contents fails (caught by an
/// audit-delete spy trigger under the seed-randomized runner). Every test
/// below that runs a fixture array snapshots the rows first and defers the
/// restore. Row snapshot/re-insert — NOT a canonical re-apply: applied
/// migrations are skipped via these very rows, so re-running the SQL against
/// a live schema collides on unguarded DDL (e.g. v9's CREATE TRIGGER).
const SavedMigrationRows = struct {
    // 64 slots ≥ the canonical set with headroom; stack-only, no heap.
    versions: [64]i32 = @splat(0),
    applied: [64]i64 = @splat(0),
    len: usize = 0,
};

fn snapshotAuditRows(conn: *Conn) !SavedMigrationRows {
    var saved: SavedMigrationRows = .{};
    var q = PgQuery.from(try conn.query(
        "SELECT version, applied_at FROM audit.schema_migrations ORDER BY version",
        .{},
    ));
    defer q.deinit();
    while (try q.next()) |row| {
        if (saved.len >= saved.versions.len) break;
        saved.versions[saved.len] = try row.get(i32, 0);
        saved.applied[saved.len] = try row.get(i64, 1);
        saved.len += 1;
    }
    return saved;
}

fn restoreAuditRows(conn: *Conn, saved: *const SavedMigrationRows) void {
    for (0..saved.len) |i| {
        _ = conn.exec(
            "INSERT INTO audit.schema_migrations (version, applied_at) VALUES ($1, $2) ON CONFLICT (version) DO NOTHING",
            .{ saved.versions[i], saved.applied[i] },
        ) catch |err| std.log.warn("audit row restore failed: {s}", .{@errorName(err)});
    }
}

const Conn = pool_mod.Conn;
const parseUrl = pool_mod.parseUrl;
const roleEnvVarName = pool_mod.roleEnvVarName;
const TEST_RUN_MS: i64 = 1_000;

test "parseUrl parses host, port, db, credentials" {
    const alloc = std.testing.allocator;
    const opts = try parseUrl(alloc, "postgres://alice:secret@localhost:5433/agentsfleetdb");
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
    try std.testing.expectEqualStrings("agentsfleetdb", database);
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
    const opts = try parseUrl(alloc, "postgres://u:p@host:5432/agentsfleetdb?sslmode=require");
    defer alloc.free(opts.connect.host.?);
    defer alloc.free(opts.auth.username);
    defer alloc.free(opts.auth.password.?);
    defer alloc.free(opts.auth.database.?);

    try std.testing.expectEqualStrings("agentsfleetdb", opts.auth.database.?);
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
    try std.testing.expectEqualStrings("DATABASE_URL_MIGRATOR", roleEnvVarName(.migrator));
}

test "DbRole carries no worker variant" {
    inline for (@typeInfo(pool_mod.DbRole).@"enum".fields) |field| {
        try std.testing.expect(!std.mem.eql(u8, field.name, "worker"));
    }
}

fn openIntegrationTestConn(alloc: std.mem.Allocator) !?test_fixtures.TestConnCtx {
    // DB-backed integration tests must be opt-in via TEST_DATABASE_URL —
    // gate BEFORE delegating so the shared fixture's DATABASE_URL fallback
    // can never pull an unrelated unit-lane database in.
    if (env.testLiveValue("TEST_DATABASE_URL") == null) return null;
    // Shared assembly (Dimension 6.3); pool/acquire failures skip, not fail.
    return test_fixtures.openTestConn(alloc) catch null;
}

test "integration: canary pool acquire + exec + query SELECT 1" {
    const alloc = std.testing.allocator;
    const url = env.testLiveValue("TEST_DATABASE_URL") orelse
        env.testLiveValue("DATABASE_URL") orelse return error.SkipZigTest;

    const opts = try parseUrl(std.heap.page_allocator, url);
    const inner = try pg.Pool.init(@import("common").globalIo(), alloc, opts);
    const pool = try pool_mod.adopt(inner, alloc);

    defer pool.deinit();

    const conn = try pool.acquire();
    defer pool.release(conn);

    // Simple query protocol
    _ = try conn.exec("SELECT 1", .{});
    // Extended query protocol (no params)
    _ = try conn.exec("SELECT 1", .{});
}

test "T6 integration: generated UUID PKs round-trip through INSERT and SELECT" {
    if (env.testLiveValue("LIVE_DB") == null) return error.SkipZigTest;
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
    const tid = try id_format.allocUuidV7(alloc);
    defer alloc.free(tid);
    const row_id = try id_format.allocUuidV7(alloc);
    defer alloc.free(row_id);
    const pid = try id_format.allocUuidV7(alloc);
    defer alloc.free(pid);

    _ = try db_ctx.conn.exec(
        "INSERT INTO t6_run_transitions (id, run_id, ts) VALUES ($1::uuid, 'run-1', $2)",
        .{ tid, TEST_RUN_MS },
    );
    _ = try db_ctx.conn.exec(
        "INSERT INTO t6_usage_ledger (id, run_id) VALUES ($1::uuid, 'run-1')",
        .{row_id},
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
            .{row_id},
        ));
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqualStrings(row_id, try row.get([]const u8, 0));
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
    if (env.testLiveValue("LIVE_DB") == null) return error.SkipZigTest;
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
    if (env.testLiveValue("LIVE_DB") == null) return error.SkipZigTest;
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

    const dup_id = try id_format.allocUuidV7(alloc);
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
    if (env.testLiveValue("LIVE_DB") == null) return error.SkipZigTest;
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
    if (env.testLiveValue("LIVE_DB") == null) return error.SkipZigTest;
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
    if (env.testLiveValue("LIVE_DB") == null) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const db_ctx = (try openIntegrationTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    // `ops_ro` is not in this list: slot 100 deliberately stopped creating it —
    // the read-only operator principals are ROLES with grants, not a schema of
    // their own. `memory` is, and owns `memory.memory_entries`.
    const schema_checks = [_][]const u8{ "core", "fleet", "billing", "vault", "audit", "memory" };
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
            \\  AND table_name IN ('tenants', 'workspaces', 'runs')
            \\LIMIT 1
        , .{}));
        defer q.deinit();
        try std.testing.expect((try q.next()) == null);
    }

    const role_checks = [_][]const u8{
        "db_migrator",
        "api_runtime",
        "ops_readonly_human",
        "ops_readonly_fleet",
    };
    inline for (role_checks) |role_name| {
        var role_q = PgQuery.from(try db_ctx.conn.query(
            "SELECT 1 FROM pg_roles WHERE rolname = $1",
            .{role_name},
        ));
        defer role_q.deinit();
        try std.testing.expect((try role_q.next()) != null);
    }

    // The worker datastore role is retired: a clean migration apply must not
    // create it. The literal below is the role name we assert is absent.
    {
        var absent_q = PgQuery.from(try db_ctx.conn.query(
            "SELECT 1 FROM pg_roles WHERE rolname = $1",
            .{"worker_runtime"},
        ));
        defer absent_q.deinit();
        try std.testing.expect((try absent_q.next()) == null);
    }
}

test "integration: runMigrations is idempotent when table exists but migration record is absent" {
    if (env.testLiveValue("LIVE_DB") == null) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const db_ctx = (try openIntegrationTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    var saved_audit = try snapshotAuditRows(db_ctx.conn);
    defer restoreAuditRows(db_ctx.conn, &saved_audit);

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

test "integration: runMigrations reaps orphan rows for versions no longer in canonical list" {
    if (env.testLiveValue("LIVE_DB") == null) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const db_ctx = (try openIntegrationTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    var saved_audit = try snapshotAuditRows(db_ctx.conn);
    defer restoreAuditRows(db_ctx.conn, &saved_audit);

    const orphan_version: i32 = 99997;
    const keep_version: i32 = 99996;
    const keep_sql =
        \\CREATE TABLE IF NOT EXISTS public.test_reap_keep_fixture (id BIGINT PRIMARY KEY);
    ;
    const canonical = [_]pool_mod.Migration{
        .{ .version = keep_version, .sql = keep_sql },
    };

    // Clean slate, then seed an orphan row (simulates a migration that was removed
    // from the canonical list — e.g., M17_001's v8 and v11).
    _ = db_ctx.conn.exec("DELETE FROM audit.schema_migrations WHERE version IN ($1, $2)", .{ orphan_version, keep_version }) catch {};
    _ = db_ctx.conn.exec("DELETE FROM audit.schema_migration_failures WHERE version IN ($1, $2)", .{ orphan_version, keep_version }) catch {};
    _ = db_ctx.conn.exec("DROP TABLE IF EXISTS public.test_reap_keep_fixture", .{}) catch {};
    _ = try db_ctx.conn.exec(
        "INSERT INTO audit.schema_migrations (version, applied_at) VALUES ($1, $2)",
        .{ orphan_version, clock.nowMillis() },
    );

    // Run canonical migrations — the reap step should remove orphan_version.
    try pool_mod.runMigrations(db_ctx.pool, &canonical);

    {
        var q = PgQuery.from(try db_ctx.conn.query(
            "SELECT 1 FROM audit.schema_migrations WHERE version = $1",
            .{orphan_version},
        ));
        defer q.deinit();
        try std.testing.expect((try q.next()) == null);
    }

    {
        var q = PgQuery.from(try db_ctx.conn.query(
            "SELECT 1 FROM audit.schema_migrations WHERE version = $1",
            .{keep_version},
        ));
        defer q.deinit();
        try std.testing.expect((try q.next()) != null);
    }

    _ = db_ctx.conn.exec("DELETE FROM audit.schema_migrations WHERE version = $1", .{keep_version}) catch {};
    _ = db_ctx.conn.exec("DROP TABLE IF EXISTS public.test_reap_keep_fixture", .{}) catch {};
}

test "integration: runMigrations reaps orphan rows in schema_migration_failures (T2/T6)" {
    if (env.testLiveValue("LIVE_DB") == null) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const db_ctx = (try openIntegrationTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    var saved_audit = try snapshotAuditRows(db_ctx.conn);
    defer restoreAuditRows(db_ctx.conn, &saved_audit);

    const orphan_version: i32 = 99995;
    const keep_version: i32 = 99994;
    const keep_sql =
        \\CREATE TABLE IF NOT EXISTS public.test_reap_failures_fixture (id BIGINT PRIMARY KEY);
    ;
    const canonical = [_]pool_mod.Migration{
        .{ .version = keep_version, .sql = keep_sql },
    };

    _ = db_ctx.conn.exec("DELETE FROM audit.schema_migrations WHERE version IN ($1, $2)", .{ orphan_version, keep_version }) catch {};
    _ = db_ctx.conn.exec("DELETE FROM audit.schema_migration_failures WHERE version IN ($1, $2)", .{ orphan_version, keep_version }) catch {};
    _ = db_ctx.conn.exec("DROP TABLE IF EXISTS public.test_reap_failures_fixture", .{}) catch {};

    // Seed an orphan failure row — simulates a previously-failed migration that
    // has since been removed from the canonical list.
    _ = try db_ctx.conn.exec(
        \\INSERT INTO audit.schema_migration_failures (version, failed_at, error_text)
        \\VALUES ($1, $2, 'simulated')
    , .{ orphan_version, clock.nowMillis() });

    try pool_mod.runMigrations(db_ctx.pool, &canonical);

    var q = PgQuery.from(try db_ctx.conn.query(
        "SELECT 1 FROM audit.schema_migration_failures WHERE version = $1",
        .{orphan_version},
    ));
    defer q.deinit();
    try std.testing.expect((try q.next()) == null);

    _ = db_ctx.conn.exec("DELETE FROM audit.schema_migrations WHERE version = $1", .{keep_version}) catch {};
    _ = db_ctx.conn.exec("DROP TABLE IF EXISTS public.test_reap_failures_fixture", .{}) catch {};
}

test "integration: runMigrations reap is a no-op when all applied rows are canonical (T2)" {
    if (env.testLiveValue("LIVE_DB") == null) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const db_ctx = (try openIntegrationTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    var saved_audit = try snapshotAuditRows(db_ctx.conn);
    defer restoreAuditRows(db_ctx.conn, &saved_audit);

    const v1: i32 = 99993;
    const v2: i32 = 99992;
    const sql_a =
        \\CREATE TABLE IF NOT EXISTS public.test_reap_noop_a (id BIGINT PRIMARY KEY);
    ;
    const sql_b =
        \\CREATE TABLE IF NOT EXISTS public.test_reap_noop_b (id BIGINT PRIMARY KEY);
    ;
    const canonical = [_]pool_mod.Migration{
        .{ .version = v1, .sql = sql_a },
        .{ .version = v2, .sql = sql_b },
    };

    _ = db_ctx.conn.exec("DELETE FROM audit.schema_migrations WHERE version IN ($1, $2)", .{ v1, v2 }) catch {};
    _ = db_ctx.conn.exec("DROP TABLE IF EXISTS public.test_reap_noop_a", .{}) catch {};
    _ = db_ctx.conn.exec("DROP TABLE IF EXISTS public.test_reap_noop_b", .{}) catch {};

    try pool_mod.runMigrations(db_ctx.pool, &canonical);

    // Second run — reap must preserve all canonical rows (no-op).
    try pool_mod.runMigrations(db_ctx.pool, &canonical);

    {
        var q = PgQuery.from(try db_ctx.conn.query(
            "SELECT COUNT(*)::BIGINT FROM audit.schema_migrations WHERE version IN ($1, $2)",
            .{ v1, v2 },
        ));
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        const count = try row.get(i64, 0);
        try std.testing.expectEqual(@as(i64, 2), count);
    }

    _ = db_ctx.conn.exec("DELETE FROM audit.schema_migrations WHERE version IN ($1, $2)", .{ v1, v2 }) catch {};
    _ = db_ctx.conn.exec("DROP TABLE IF EXISTS public.test_reap_noop_a", .{}) catch {};
    _ = db_ctx.conn.exec("DROP TABLE IF EXISTS public.test_reap_noop_b", .{}) catch {};
}

test "integration: runMigrations succeeds on empty migrations list (no SQL syntax error)" {
    // Regression: pre-fix the reap helper built `DELETE … WHERE version NOT
    // IN ()` which Postgres rejects with sqlstate 42601 (`syntax error at
    // or near ")"`). An empty migrations slice must early-return cleanly.
    if (env.testLiveValue("LIVE_DB") == null) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const db_ctx = (try openIntegrationTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    const empty: []const pool_mod.Migration = &[_]pool_mod.Migration{};
    try pool_mod.runMigrations(db_ctx.pool, empty);
}

test "integration: readonly roles cannot read vault.secrets" {
    if (env.testLiveValue("LIVE_DB") == null) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const db_ctx = (try openIntegrationTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    var q = PgQuery.from(try db_ctx.conn.query(
        "SELECT has_table_privilege('ops_readonly_fleet', 'vault.secrets', 'SELECT')",
        .{},
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    const can_read_vault = try row.get(bool, 0);
    try std.testing.expect(!can_read_vault);
}

// Grant-equivalence regression for the worker-substrate retirement: api_runtime
// is the sole data-plane role, and the lease/report path INSERTs/UPDATEs these
// tables through the api pool. INSERT+UPDATE on fleet_sessions and fleet_events
// formerly lived only on the removed worker role; the collapse must move them
// onto api_runtime. has_table_privilege evaluates the named role directly, so
// this proves the grant without a superuser bypass (the real statements are
// exercised by the fleet integration suite).
test "integration: api_runtime holds the fleet lease/report write grants" {
    if (env.testLiveValue("LIVE_DB") == null) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const db_ctx = (try openIntegrationTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    // The lease/report write set api_runtime reaches UNELEVATED: the per-event
    // lifecycle tables. fleet_sessions + fleet_events were the two formerly
    // granted to worker_runtime only — the rest api_runtime always held.
    //
    // The two money tables are deliberately absent. They moved behind roles the
    // handler must assume, so their reachability is asserted below instead of
    // here; leaving them in this list would have kept passing while the fence
    // it now proves did not exist.
    const write_set = [_][]const u8{
        "core.fleets",
        "core.fleet_events",
        "core.fleet_sessions",
        "core.fleet_approval_gates",
    };
    inline for (write_set) |tbl| {
        var q = PgQuery.from(try db_ctx.conn.query(
            "SELECT has_table_privilege('api_runtime', $1, 'SELECT'), " ++
                "has_table_privilege('api_runtime', $1, 'INSERT'), " ++
                "has_table_privilege('api_runtime', $1, 'UPDATE')",
            .{tbl},
        ));
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        try std.testing.expect(try row.get(bool, 0)); // SELECT
        try std.testing.expect(try row.get(bool, 1)); // INSERT
        try std.testing.expect(try row.get(bool, 2)); // UPDATE
    }
}

// The role/table privilege matrix, asserted from the catalogue rather than from
// the grant text. `has_table_privilege` answers what a role can actually DO, so
// it accounts for membership and inheritance that reading the GRANT statements
// cannot.
//
// Two-sided on purpose: every row states BOTH what the role may do and what it
// may not, because a one-sided check cannot tell "the boundary holds" from "the
// grant was dropped". The `crypto_store` and metering suites exercise every one
// of these statements already, but they connect as the database OWNER, which
// bypasses grants entirely — so they stay green whether or not the runtime role
// can reach the table. This asks the question a superuser connection cannot
// answer by accident.
//
// `db_migrator` is deliberately absent: its authority comes from OWNING the
// tables, and the owning role is a deployment property (locally the superuser
// that runs compose owns them), not something this schema grants. Pinning it
// here would pin a local artifact.
const RolePrivilege = struct {
    role: []const u8,
    table: []const u8,
    select: bool,
    insert: bool,
    update: bool,
    delete: bool,
};

const ROLE_PRIVILEGE_MATRIX = [_]RolePrivilege{
    // api_runtime — every Hypertext Transfer Protocol handler runs as this role.
    // The secret store and the wallet sit behind `vault_runtime` and
    // `billing_runtime` (schema/110, 300, 700): api_runtime's membership in
    // both is non-inheriting, dormant until a statement runs inside
    // db/pool_elevation.zig's SET ROLE scope. has_table_privilege evaluates
    // the named role without SET ROLE, so these zeros pin the dormant state.
    .{ .role = "api_runtime", .table = "vault.secrets", .select = false, .insert = false, .update = false, .delete = false },
    .{ .role = "api_runtime", .table = "billing.tenant_wallet", .select = false, .insert = false, .update = false, .delete = false },
    // The charge history stays readable unelevated — its list endpoints page
    // through it — but writes belong to `billing_runtime`, and DELETE to
    // nobody: a charge leaves only with the tenant that paid, via the cascade.
    .{ .role = "api_runtime", .table = "billing.usage_ledger", .select = true, .insert = false, .update = false, .delete = false },
    // Memory is behind `memory_runtime`; api_runtime must SET ROLE to reach it.
    .{ .role = "api_runtime", .table = "memory.memory_entries", .select = false, .insert = false, .update = false, .delete = false },

    // memory_runtime — the memory elevation role reaches memory ONLY.
    .{ .role = "memory_runtime", .table = "memory.memory_entries", .select = true, .insert = true, .update = true, .delete = true },
    .{ .role = "memory_runtime", .table = "vault.secrets", .select = false, .insert = false, .update = false, .delete = false },
    .{ .role = "memory_runtime", .table = "billing.tenant_wallet", .select = false, .insert = false, .update = false, .delete = false },
    .{ .role = "memory_runtime", .table = "billing.usage_ledger", .select = false, .insert = false, .update = false, .delete = false },

    // vault_runtime — the sealed store, and nothing that holds money.
    .{ .role = "vault_runtime", .table = "vault.secrets", .select = true, .insert = true, .update = true, .delete = true },
    .{ .role = "vault_runtime", .table = "billing.tenant_wallet", .select = false, .insert = false, .update = false, .delete = false },
    .{ .role = "vault_runtime", .table = "billing.usage_ledger", .select = false, .insert = false, .update = false, .delete = false },

    // billing_runtime — the wallet and the ledger. No DELETE on either: a
    // wallet leaves with its tenant through the cascade, and a charge never
    // leaves at all.
    .{ .role = "billing_runtime", .table = "billing.tenant_wallet", .select = true, .insert = true, .update = true, .delete = false },
    .{ .role = "billing_runtime", .table = "billing.usage_ledger", .select = true, .insert = true, .update = true, .delete = false },
    .{ .role = "billing_runtime", .table = "vault.secrets", .select = false, .insert = false, .update = false, .delete = false },

    // metering_runtime — the composite, and the row that proves it is composed
    // rather than inherited: it reads and updates the wallet, but may not
    // CREATE one. An inheriting membership in billing_runtime would silently
    // flip that INSERT to true, which is exactly the widening this pins shut.
    .{ .role = "metering_runtime", .table = "billing.tenant_wallet", .select = true, .insert = false, .update = true, .delete = false },
    .{ .role = "metering_runtime", .table = "billing.usage_ledger", .select = true, .insert = true, .update = true, .delete = false },
    .{ .role = "metering_runtime", .table = "fleet.runner_leases", .select = true, .insert = false, .update = true, .delete = false },
    .{ .role = "metering_runtime", .table = "vault.secrets", .select = false, .insert = false, .update = false, .delete = false },

    // Read-only operator principals reach neither money nor secrets, in any
    // direction. Each schema slot REVOKEs explicitly so re-widening is a visible
    // edit; these rows are what makes that revoke provable.
    .{ .role = "ops_readonly_human", .table = "vault.secrets", .select = false, .insert = false, .update = false, .delete = false },
    .{ .role = "ops_readonly_human", .table = "billing.tenant_wallet", .select = false, .insert = false, .update = false, .delete = false },
    .{ .role = "ops_readonly_human", .table = "billing.usage_ledger", .select = false, .insert = false, .update = false, .delete = false },
    .{ .role = "ops_readonly_fleet", .table = "vault.secrets", .select = false, .insert = false, .update = false, .delete = false },
    .{ .role = "ops_readonly_fleet", .table = "billing.tenant_wallet", .select = false, .insert = false, .update = false, .delete = false },
    .{ .role = "ops_readonly_fleet", .table = "billing.usage_ledger", .select = false, .insert = false, .update = false, .delete = false },
};

fn expectPrivilege(
    conn: *Conn,
    role: []const u8,
    table: []const u8,
    privilege: []const u8,
    want: bool,
) !void {
    var q = PgQuery.from(try conn.query(
        "SELECT has_table_privilege($1, $2, $3)",
        .{ role, table, privilege },
    ));
    defer q.deinit();
    const row = (try q.next()) orelse return error.TestUnexpectedResult;
    const got = try row.get(bool, 0);
    if (got != want) {
        std.debug.print(
            "\nFAIL: {s} may {s} {s} — expected {}, found {}\n",
            .{ role, privilege, table, want, got },
        );
        return error.TestUnexpectedResult;
    }
}

test "integration: the role/table privilege matrix holds in both directions" {
    if (env.testLiveValue("LIVE_DB") == null) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const db_ctx = (try openIntegrationTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    for (ROLE_PRIVILEGE_MATRIX) |row| {
        try expectPrivilege(db_ctx.conn, row.role, row.table, "SELECT", row.select);
        try expectPrivilege(db_ctx.conn, row.role, row.table, "INSERT", row.insert);
        try expectPrivilege(db_ctx.conn, row.role, row.table, "UPDATE", row.update);
        try expectPrivilege(db_ctx.conn, row.role, row.table, "DELETE", row.delete);
    }
}

// Executed, not declared. `has_table_privilege` answers what the catalogue
// SAYS; this runs the statements and reads what PostgreSQL DOES. The two can
// disagree — a column-scoped grant satisfies a table-level SELECT check while
// every decrypt still fails — and the disagreement is exactly the shape that
// shipped a schema whose own tests passed while signup was refused.
//
// Both directions on the same connection: the writes that must succeed, then
// the reads that must be refused. A denial is asserted as an error from the
// statement, not as a catalogue answer, so it cannot pass by mis-reading the
// question.
const PRIV_WS = "0195b4ba-8d3a-7f13-8abc-0000000009e1";
const PRIV_SECRET_ID = "0195b4ba-8d3a-7f13-8abc-0000000009e2";

test "integration: role-scoped statements succeed and are refused as the matrix declares" {
    if (env.testLiveValue("LIVE_DB") == null) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const db_ctx = (try openIntegrationTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);
    const conn = db_ctx.conn;

    // Seeded as the owner; the role-scoped work below is what is under test.
    try test_fixtures.seedTenant(conn);
    try test_fixtures.seedWorkspace(conn, PRIV_WS);
    defer test_fixtures.teardownTenant(conn);
    defer test_fixtures.teardownWorkspace(conn, PRIV_WS);
    defer _ = conn.exec("RESET ROLE", .{}) catch {};
    defer _ = conn.exec("DELETE FROM vault.secrets WHERE workspace_id = $1::uuid", .{PRIV_WS}) catch {};
    defer _ = conn.exec("DELETE FROM billing.tenant_wallet WHERE tenant_id = $1::uuid", .{test_fixtures.TEST_TENANT_ID}) catch {};

    // ── billing_runtime: the money path must WORK under elevation ──────────
    // The wallet writers reach these statements through db/pool_elevation.zig,
    // never as bare api_runtime — so the role probed here is the owning one.
    _ = try conn.exec("SET ROLE billing_runtime", .{});

    // The write that refuses every signup when the grant is missing:
    // insertStarterGrant runs it inside the tenant-create transaction.
    _ = try conn.exec(
        \\INSERT INTO billing.tenant_wallet
        \\  (tenant_id, balance_nanos, grant_source, created_at, updated_at)
        \\VALUES ($1::uuid, 500, 'starter', 0, 0)
        \\ON CONFLICT (tenant_id) DO NOTHING
    , .{test_fixtures.TEST_TENANT_ID});
    // The write every metered debit performs.
    _ = try conn.exec(
        "UPDATE billing.tenant_wallet SET balance_nanos = balance_nanos - 1 WHERE tenant_id = $1::uuid",
        .{test_fixtures.TEST_TENANT_ID},
    );
    _ = try conn.exec("RESET ROLE", .{});

    // ── vault_runtime: the secret path must WORK under elevation ───────────
    _ = try conn.exec("SET ROLE vault_runtime", .{});
    // The secret write, and a read of the sealed bytes back — a metadata-only
    // column grant would pass the INSERT and fail this SELECT.
    _ = try conn.exec(
        \\INSERT INTO vault.secrets
        \\  (id, workspace_id, key_name, encrypted_dek, dek_nonce, dek_tag,
        \\   nonce, ciphertext, tag, kek_version, created_at, updated_at)
        \\VALUES ($1::uuid, $2::uuid, 'privilege-probe', 'x', 'x', 'x', 'x', 'x', 'x', 1, 0, 0)
        \\ON CONFLICT DO NOTHING
    , .{ PRIV_SECRET_ID, PRIV_WS });
    {
        var q = PgQuery.from(try conn.query(
            "SELECT ciphertext FROM vault.secrets WHERE workspace_id = $1::uuid",
            .{PRIV_WS},
        ));
        defer q.deinit();
        try std.testing.expect((try q.next()) != null);
    }

    _ = try conn.exec("RESET ROLE", .{});

    // ── read-only operators: the same reads must be REFUSED ────────────────
    // Asserted as a statement error. `expectError` is not used: the driver
    // reports a refusal as the generic `error.PG`, so naming that specific
    // error would pin the driver's error mapping rather than the refusal.
    inline for (.{ "ops_readonly_human", "ops_readonly_fleet" }) |role| {
        _ = try conn.exec("SET ROLE " ++ role, .{});
        if (conn.exec("SELECT ciphertext FROM vault.secrets WHERE workspace_id = $1::uuid", .{PRIV_WS})) |_| {
            std.debug.print("\n\nFAIL: {s} read vault.secrets — the REVOKE in schema/300 is not holding\n", .{role});
            _ = conn.exec("RESET ROLE", .{}) catch {};
            return error.TestUnexpectedResult;
        } else |_| {}
        _ = try conn.exec("RESET ROLE", .{});

        _ = try conn.exec("SET ROLE " ++ role, .{});
        if (conn.exec("SELECT balance_nanos FROM billing.tenant_wallet WHERE tenant_id = $1::uuid", .{test_fixtures.TEST_TENANT_ID})) |_| {
            std.debug.print("\n\nFAIL: {s} read billing.tenant_wallet — the REVOKE in schema/700 is not holding\n", .{role});
            _ = conn.exec("RESET ROLE", .{}) catch {};
            return error.TestUnexpectedResult;
        } else |_| {}
        _ = try conn.exec("RESET ROLE", .{});
    }
}

// The envelope columns specifically. A column-scoped grant covering only the
// metadata projection satisfies every table-level SELECT asserted above while
// every decrypt path still fails, so the table grants alone do not prove the
// read path works — this is the assertion that does. `vault_runtime` is the
// role every decrypt runs as; api_runtime's zero-grant state is pinned by
// db/schema_privilege_integration_test.zig.
test "integration: vault_runtime can read the sealed vault columns, not just metadata" {
    if (env.testLiveValue("LIVE_DB") == null) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const db_ctx = (try openIntegrationTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    const sealed = [_][]const u8{ "ciphertext", "encrypted_dek", "dek_nonce", "dek_tag", "nonce", "tag", "kek_version" };
    for (sealed) |column| {
        var q = PgQuery.from(try db_ctx.conn.query(
            "SELECT has_column_privilege($1, 'vault.secrets', $2, 'SELECT')",
            .{ "vault_runtime", column },
        ));
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        if (!(try row.get(bool, 0))) {
            std.debug.print("\nFAIL: vault_runtime cannot read vault.secrets.{s}\n", .{column});
            return error.TestUnexpectedResult;
        }
    }
}

// ── Migration advisory lock: retry decision + real-DB concurrency ──────────

test "unit: migration lock retry decision (classifyAttempt)" {
    const ml = migration_lock;
    try std.testing.expectEqual(ml.Outcome.acquired, ml.classifyAttempt(true, 1, 3));
    try std.testing.expectEqual(ml.Outcome.acquired, ml.classifyAttempt(true, 3, 3)); // acquired wins at the bound
    try std.testing.expectEqual(ml.Outcome.retry, ml.classifyAttempt(false, 1, 3));
    try std.testing.expectEqual(ml.Outcome.retry, ml.classifyAttempt(false, 2, 3));
    try std.testing.expectEqual(ml.Outcome.exhausted, ml.classifyAttempt(false, 3, 3)); // bound reached
    try std.testing.expectEqual(ml.Outcome.exhausted, ml.classifyAttempt(false, 9, 3)); // past the bound
}

// Concurrency proof: while session A holds the migration advisory lock, a
// second session B must FAIL FAST (bounded) rather than block forever — the
// exact hang this module exists to prevent. Two independent pools = two real
// Postgres sessions; advisory locks are per-session.
test "integration: migration lock serializes — second session fails fast, no hang" {
    if (env.testLiveValue("LIVE_DB") == null) return error.SkipZigTest;
    const alloc = std.testing.allocator;

    const a = (try openIntegrationTestConn(alloc)) orelse return error.SkipZigTest;
    defer a.pool.deinit();
    defer a.pool.release(a.conn);
    const b = (try openIntegrationTestConn(alloc)) orelse return error.SkipZigTest;
    defer b.pool.deinit();
    defer b.pool.release(b.conn);

    // Session A takes the lock.
    try migration_lock.acquireBounded(a.conn, 3, 5);

    // Session B must fail fast with MigrationLockUnavailable — bounded, not hung.
    const t0 = clock.nowMillis();
    try std.testing.expectError(error.MigrationLockUnavailable, migration_lock.acquireBounded(b.conn, 3, 5));
    const elapsed_ms = clock.nowMillis() - t0;
    // pin test: literal is the contract — the sub-second ceiling proving "fail fast, not hang"
    try std.testing.expect(elapsed_ms < 1_000); // 3 polls × 5ms; a hang would be minutes

    // Once A releases, B acquires cleanly — proving a real lock, not a dead end.
    migration_lock.release(a.conn);
    try migration_lock.acquireBounded(b.conn, 3, 5);
    migration_lock.release(b.conn);
}

// probeAvailable is the inspect-side check (inspectMigrationState / serve-boot /
// doctor). It MUST be pooler-safe: a transaction-scoped advisory lock that
// auto-releases at statement end, so it never leaks the lock the way the old
// session-scoped tryAcquire/release pair did over a pooled connection. Prove on
// real PG: (1) free at rest, (2) detects contention while another session holds
// the migrate lock, (3) reports free again once released, and crucially (4) a
// true verdict NEVER retains the lock — another session can immediately acquire,
// which a leaking session-scoped probe would have blocked.
test "integration: probeAvailable detects contention and never leaks the lock" {
    if (env.testLiveValue("LIVE_DB") == null) return error.SkipZigTest;
    const alloc = std.testing.allocator;

    const a = (try openIntegrationTestConn(alloc)) orelse return error.SkipZigTest;
    defer a.pool.deinit();
    defer a.pool.release(a.conn);
    const b = (try openIntegrationTestConn(alloc)) orelse return error.SkipZigTest;
    defer b.pool.deinit();
    defer b.pool.release(b.conn);

    // (1) Free at rest; (4) that true probe did not retain the lock — A can take it.
    try std.testing.expect(try migration_lock.probeAvailable(b.conn));
    try migration_lock.acquireBounded(a.conn, 3, 5);

    // (2) Contention: while A holds the session lock, B's probe sees it taken.
    // Repeated probes stay false and never accumulate a held lock on B.
    try std.testing.expect(!try migration_lock.probeAvailable(b.conn));
    try std.testing.expect(!try migration_lock.probeAvailable(b.conn));

    // (3) Freed: once A releases, B's probe reports available again, and (4) the
    // lock is still free for a real acquire afterward — proving auto-release.
    migration_lock.release(a.conn);
    try std.testing.expect(try migration_lock.probeAvailable(b.conn));
    try migration_lock.acquireBounded(a.conn, 3, 5);
    migration_lock.release(a.conn);
}
