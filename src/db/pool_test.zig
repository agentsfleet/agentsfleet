const std = @import("std");
const pg = @import("pg");
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

test "roleEnvVarName maps db roles deterministically" {
    try std.testing.expectEqualStrings("DATABASE_URL", roleEnvVarName(.default));
    try std.testing.expectEqualStrings("DATABASE_URL_API", roleEnvVarName(.api));
    try std.testing.expectEqualStrings("DATABASE_URL_WORKER", roleEnvVarName(.worker));
    try std.testing.expectEqualStrings("DATABASE_URL_CALLBACK", roleEnvVarName(.callback));
}

fn openIntegrationTestConn(alloc: std.mem.Allocator) !?struct { pool: *Pool, conn: *Conn } {
    const url = std.process.getEnvVarOwned(alloc, "HANDLER_DB_TEST_URL") catch
        std.process.getEnvVarOwned(alloc, "DATABASE_URL") catch return null;
    defer alloc.free(url);

    var arena = std.heap.ArenaAllocator.init(alloc);
    defer arena.deinit();
    const opts = try parseUrl(arena.allocator(), url);
    const pool = pg.Pool.init(alloc, opts) catch return null;
    errdefer pool.deinit();
    const conn = pool.acquire() catch {
        pool.deinit();
        return null;
    };
    return .{ .pool = pool, .conn = conn };
}

fn createUuidContractTempSchema(conn: *Conn) !void {
    var q = try conn.query(
        \\CREATE TEMP TABLE runs (
        \\  run_id UUID PRIMARY KEY,
        \\  run_snapshot_version UUID
        \\) ON COMMIT DROP;
        \\CREATE TEMP TABLE agent_profile_versions (
        \\  profile_version_id UUID PRIMARY KEY
        \\) ON COMMIT DROP;
        \\CREATE TEMP TABLE profile_compile_jobs (
        \\  compile_job_id UUID PRIMARY KEY
        \\) ON COMMIT DROP;
        \\CREATE TEMP TABLE run_transitions (
        \\  id UUID PRIMARY KEY,
        \\  run_id UUID NOT NULL REFERENCES runs(run_id)
        \\) ON COMMIT DROP;
        \\CREATE TEMP TABLE artifacts (
        \\  run_id UUID NOT NULL REFERENCES runs(run_id)
        \\) ON COMMIT DROP;
        \\CREATE TEMP TABLE usage_ledger (
        \\  run_id UUID NOT NULL REFERENCES runs(run_id)
        \\) ON COMMIT DROP;
        \\CREATE TEMP TABLE run_side_effects (
        \\  run_id UUID NOT NULL REFERENCES runs(run_id)
        \\) ON COMMIT DROP;
        \\CREATE TEMP TABLE run_side_effect_outbox (
        \\  run_id UUID NOT NULL REFERENCES runs(run_id)
        \\) ON COMMIT DROP;
        \\CREATE TEMP TABLE workspace_memories (
        \\  run_id UUID NOT NULL REFERENCES runs(run_id)
        \\) ON COMMIT DROP;
        \\CREATE TEMP TABLE policy_events (
        \\  run_id UUID REFERENCES runs(run_id)
        \\) ON COMMIT DROP;
        \\CREATE TEMP TABLE workspace_active_profile (
        \\  profile_version_id UUID NOT NULL REFERENCES agent_profile_versions(profile_version_id)
        \\) ON COMMIT DROP;
        \\CREATE TEMP TABLE profile_linkage_audit_artifacts (
        \\  profile_version_id UUID NOT NULL REFERENCES agent_profile_versions(profile_version_id),
        \\  compile_job_id UUID REFERENCES profile_compile_jobs(compile_job_id),
        \\  run_id UUID REFERENCES runs(run_id)
        \\) ON COMMIT DROP
    , .{});
    q.deinit();
}

test "integration: canary pool acquire + exec + query SELECT 1" {
    const alloc = std.testing.allocator;
    const url = std.process.getEnvVarOwned(alloc, "HANDLER_DB_TEST_URL") catch
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
    var q = try conn.query("SELECT 1", .{});
    q.deinit();
}

test "integration: uuid contract tables are UUID typed for run/profile/linkage IDs" {
    if (!std.process.hasEnvVarConstant("LIVE_DB")) return error.SkipZigTest;
    const db_ctx = (try openIntegrationTestConn(std.testing.allocator)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try createUuidContractTempSchema(db_ctx.conn);

    {
        var q = try db_ctx.conn.query(
            \\SELECT table_name, column_name, data_type
            \\FROM information_schema.columns
            \\WHERE table_name IN ('runs', 'agent_profile_versions', 'profile_compile_jobs', 'profile_linkage_audit_artifacts')
            \\  AND column_name IN ('run_id', 'run_snapshot_version', 'profile_version_id', 'compile_job_id')
            \\ORDER BY table_name, column_name
        , .{});
        defer q.deinit();

        while (try q.next()) |row| {
            try std.testing.expectEqualStrings("uuid", try row.get([]const u8, 2));
        }
    }

    const run_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f99";
    const pver_id = "0195b4ba-8d3a-7f13-9abc-2b3e1e0a6f98";
    const cjob_id = "0195b4ba-8d3a-7f13-aabc-2b3e1e0a6f97";
    var insert_q = try db_ctx.conn.query(
        \\INSERT INTO runs (run_id, run_snapshot_version) VALUES ($1::uuid, $2::uuid);
        \\INSERT INTO agent_profile_versions (profile_version_id) VALUES ($2::uuid);
        \\INSERT INTO profile_compile_jobs (compile_job_id) VALUES ($3::uuid);
        \\INSERT INTO profile_linkage_audit_artifacts (profile_version_id, compile_job_id, run_id) VALUES ($2::uuid, $3::uuid, $1::uuid)
    , .{ run_id, pver_id, cjob_id });
    insert_q.deinit();
}

test "T6 integration: generated UUID PKs round-trip through INSERT and SELECT" {
    if (!std.process.hasEnvVarConstant("LIVE_DB")) return error.SkipZigTest;
    const alloc = std.testing.allocator;
    const db_ctx = (try openIntegrationTestConn(alloc)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    // Create TEMP tables with UUID PK + CHECK constraint (mirrors real schema)
    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE t6_run_transitions (
            \\  id UUID PRIMARY KEY,
            \\  CONSTRAINT ck_t6_rt_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
            \\  run_id TEXT NOT NULL,
            \\  ts BIGINT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE t6_usage_ledger (
            \\  id UUID PRIMARY KEY,
            \\  CONSTRAINT ck_t6_ul_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
            \\  run_id TEXT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE t6_policy_events (
            \\  id UUID PRIMARY KEY,
            \\  CONSTRAINT ck_t6_pe_uuidv7 CHECK (substring(id::text from 15 for 1) = '7'),
            \\  workspace_id TEXT NOT NULL
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }

    // INSERT with generated ids
    const tid = try id_format.generateTransitionId(alloc);
    defer alloc.free(tid);
    const uid = try id_format.generateUsageLedgerId(alloc);
    defer alloc.free(uid);
    const pid = try id_format.generatePolicyEventId(alloc);
    defer alloc.free(pid);

    {
        var q = try db_ctx.conn.query(
            "INSERT INTO t6_run_transitions (id, run_id, ts) VALUES ($1::uuid, 'run-1', 1000)",
            .{tid},
        );
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            "INSERT INTO t6_usage_ledger (id, run_id) VALUES ($1::uuid, 'run-1')",
            .{uid},
        );
        q.deinit();
    }
    {
        var q = try db_ctx.conn.query(
            "INSERT INTO t6_policy_events (id, workspace_id) VALUES ($1::uuid, 'ws-1')",
            .{pid},
        );
        q.deinit();
    }

    // SELECT and verify round-trip: id::text matches original string
    {
        var q = try db_ctx.conn.query(
            "SELECT id::text FROM t6_run_transitions WHERE id = $1::uuid",
            .{tid},
        );
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqualStrings(tid, try row.get([]const u8, 0));
    }
    {
        var q = try db_ctx.conn.query(
            "SELECT id::text FROM t6_usage_ledger WHERE id = $1::uuid",
            .{uid},
        );
        defer q.deinit();
        const row = (try q.next()) orelse return error.TestUnexpectedResult;
        try std.testing.expectEqualStrings(uid, try row.get([]const u8, 0));
    }
    {
        var q = try db_ctx.conn.query(
            "SELECT id::text FROM t6_policy_events WHERE id = $1::uuid",
            .{pid},
        );
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

    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE t6_check_reject (
            \\  id UUID PRIMARY KEY,
            \\  CONSTRAINT ck_t6_cr_uuidv7 CHECK (substring(id::text from 15 for 1) = '7')
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }

    // v4 UUID must be rejected by the CHECK constraint
    try std.testing.expectError(error.PgError, db_ctx.conn.query(
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

    {
        var q = try db_ctx.conn.query(
            \\CREATE TEMP TABLE t6_dup_reject (
            \\  id UUID PRIMARY KEY,
            \\  CONSTRAINT ck_t6_dr_uuidv7 CHECK (substring(id::text from 15 for 1) = '7')
            \\) ON COMMIT DROP
        , .{});
        q.deinit();
    }

    const dup_id = try id_format.generateTransitionId(alloc);
    defer alloc.free(dup_id);

    {
        var q = try db_ctx.conn.query(
            "INSERT INTO t6_dup_reject (id) VALUES ($1::uuid)",
            .{dup_id},
        );
        q.deinit();
    }
    // Second insert with same id must fail (PK violation)
    try std.testing.expectError(error.PgError, db_ctx.conn.query(
        "INSERT INTO t6_dup_reject (id) VALUES ($1::uuid)",
        .{dup_id},
    ));
}
