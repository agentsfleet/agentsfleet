const db = @import("../db/pool.zig");
const std = @import("std");
const common = @import("common");
const logging = @import("log");
const log = logging.scoped(.agentsfleetd);

const EnvMap = common.env.Map;

pub const MigrationGuardError = error{
    InvalidMigrateOnStart,
    MigrationPending,
    MigrationFailed,
    MigrationSchemaAhead,
    MigrationLockUnavailable,
};

const ServeMigrationDecision = enum {
    allow_without_running,
    run_required,
};

const schema_mod = @import("schema");
const schema_migrations = schema_mod.migrations;

pub fn canonicalMigrations() [schema_migrations.len]db.Migration {
    var result: [schema_migrations.len]db.Migration = undefined;
    for (schema_migrations, 0..) |m, i| {
        result[i] = .{ .version = m.version, .sql = m.sql };
    }
    return result;
}

pub fn migrateOnStartEnabledFromEnv(env_map: *const EnvMap, alloc: std.mem.Allocator) !bool {
    const raw = (try common.env.owned(env_map, alloc, "MIGRATE_ON_START")) orelse return false;
    defer alloc.free(raw);

    if (std.mem.eql(u8, raw, "1")) return true;
    if (std.mem.eql(u8, raw, "0")) return false;
    if (std.ascii.eqlIgnoreCase(raw, "true")) return true;
    if (std.ascii.eqlIgnoreCase(raw, "false")) return false;

    return MigrationGuardError.InvalidMigrateOnStart;
}

fn decideServeMigrationPolicy(
    state: db.MigrationState,
    migrate_on_start: bool,
) MigrationGuardError!ServeMigrationDecision {
    if (state.has_newer_schema_version) return MigrationGuardError.MigrationSchemaAhead;
    if (state.has_failed_migrations) return MigrationGuardError.MigrationFailed;

    if (state.applied_versions < state.expected_versions) {
        if (!migrate_on_start) return MigrationGuardError.MigrationPending;
        if (!state.lock_available) return MigrationGuardError.MigrationLockUnavailable;
        return .run_required;
    }

    return .allow_without_running;
}

/// The migration RUN must use a direct/session connection (the migrator role →
/// PlanetScale :5432), never the pooled API role (:6432, transaction mode).
/// `pg_advisory_lock` is session-scoped: over a transaction pooler the lock is
/// taken on one backend and the next statement routes to another, so the
/// migrator runs unserialized and orphans the lock on a pooled backend.
/// Inspection stays on the pooled API pool — `inspectMigrationState` uses a
/// transaction-scoped probe (`probeAvailable`) that is pooler-safe.
const MIGRATION_RUN_ROLE: db.DbRole = .migrator;

/// `pool` is the pooled API connection used for the (pooler-safe) inspection.
/// When a run is required, the actual migration is executed over a dedicated
/// session-scoped migrator pool — see MIGRATION_RUN_ROLE — opened only on that
/// path so serve needs migrator creds only when it actually auto-migrates.
pub fn enforceServeMigrationSafety(
    io: std.Io,
    env_map: *const EnvMap,
    alloc: std.mem.Allocator,
    pool: *db.Pool,
    migrate_on_start: bool,
) (MigrationGuardError || anyerror)!void {
    const migrations = canonicalMigrations();
    const state = try db.inspectMigrationState(pool, &migrations);
    const decision = try decideServeMigrationPolicy(state, migrate_on_start);

    switch (decision) {
        .allow_without_running => return,
        .run_required => {
            log.warn("startup.migration_auto_apply_start", .{ .reason = "MIGRATE_ON_START enabled" });

            // Run over a session-scoped migrator pool, NOT the pooled API pool
            // the inspection used — the session advisory lock leaks on a
            // transaction pooler (:6432). See MIGRATION_RUN_ROLE.
            const migrator_pool = try db.initFromEnvForRole(io, env_map, alloc, MIGRATION_RUN_ROLE);
            defer migrator_pool.deinit();
            try db.runMigrations(migrator_pool, &migrations);

            const post = try db.inspectMigrationState(pool, &migrations);
            if (post.has_newer_schema_version) return MigrationGuardError.MigrationSchemaAhead;
            if (post.has_failed_migrations) return MigrationGuardError.MigrationFailed;
            if (post.applied_versions < post.expected_versions) return MigrationGuardError.MigrationPending;
        },
    }
}

pub fn runCanonicalMigrations(pool: *db.Pool) !void {
    const migrations = canonicalMigrations();
    try db.runMigrations(pool, &migrations);
}

test "canonical migrations: connector install + channel tables registered (v25, v26)" {
    const migrations = canonicalMigrations();
    try std.testing.expectEqual(@as(usize, 26), migrations.len);
    var has_installs = false;
    var has_channels = false;
    for (migrations) |m| {
        if (m.version == 25) has_installs = true;
        if (m.version == 26) has_channels = true;
    }
    try std.testing.expect(has_installs);
    try std.testing.expect(has_channels);
}

test "migrateOnStartEnabledFromEnv parses known values" {
    const alloc = std.testing.allocator;
    var empty = try common.env.fromPairs(alloc, &.{});
    defer empty.deinit();
    try std.testing.expect(!try migrateOnStartEnabledFromEnv(&empty, alloc));

    var enabled = try common.env.fromPairs(alloc, &.{.{ "MIGRATE_ON_START", "1" }});
    defer enabled.deinit();
    try std.testing.expect(try migrateOnStartEnabledFromEnv(&enabled, alloc));

    var disabled = try common.env.fromPairs(alloc, &.{.{ "MIGRATE_ON_START", "false" }});
    defer disabled.deinit();
    try std.testing.expect(!try migrateOnStartEnabledFromEnv(&disabled, alloc));

    var invalid = try common.env.fromPairs(alloc, &.{.{ "MIGRATE_ON_START", "bad" }});
    defer invalid.deinit();
    try std.testing.expectError(MigrationGuardError.InvalidMigrateOnStart, migrateOnStartEnabledFromEnv(&invalid, alloc));
}

test "unit: migration guard allows startup when schema is clean" {
    const decision = try decideServeMigrationPolicy(.{
        .expected_versions = 11,
        .applied_versions = 11,
        .latest_expected_version = 15,
        .latest_applied_version = 15,
        .has_failed_migrations = false,
        .lock_available = true,
        .has_newer_schema_version = false,
    }, false);
    try std.testing.expectEqual(.allow_without_running, decision);
}

test "unit: serve auto-migrate runs over the session-scoped migrator role, never the pooled api" {
    // Regression guard for the pooled-migrator leak: serve's MIGRATE_ON_START
    // auto-apply previously ran the session-scoped advisory lock over the
    // pooled API connection (:6432), where it leaks per-transaction. The run
    // must use the direct/session migrator role (:5432).
    try std.testing.expectEqual(db.DbRole.migrator, MIGRATION_RUN_ROLE);
    try std.testing.expect(MIGRATION_RUN_ROLE != db.DbRole.api);
}

test "integration: startup allows clean schema with no pending migrations" {
    const decision = try decideServeMigrationPolicy(.{
        .expected_versions = 11,
        .applied_versions = 11,
        .latest_expected_version = 15,
        .latest_applied_version = 15,
        .has_failed_migrations = false,
        .lock_available = true,
        .has_newer_schema_version = false,
    }, false);
    try std.testing.expectEqual(.allow_without_running, decision);
}

test "integration: startup blocks when migrations are pending and MIGRATE_ON_START disabled" {
    try std.testing.expectError(MigrationGuardError.MigrationPending, decideServeMigrationPolicy(.{
        .expected_versions = 10,
        .applied_versions = 6,
        .latest_expected_version = 14,
        .latest_applied_version = 6,
        .has_failed_migrations = false,
        .lock_available = true,
        .has_newer_schema_version = false,
    }, false));
}

test "integration: startup blocks when partial failed migration state exists" {
    try std.testing.expectError(MigrationGuardError.MigrationFailed, decideServeMigrationPolicy(.{
        .expected_versions = 10,
        .applied_versions = 6,
        .latest_expected_version = 14,
        .latest_applied_version = 6,
        .has_failed_migrations = true,
        .lock_available = true,
        .has_newer_schema_version = false,
    }, true));
}

test "integration: startup blocks on concurrent migration race when lock unavailable" {
    try std.testing.expectError(MigrationGuardError.MigrationLockUnavailable, decideServeMigrationPolicy(.{
        .expected_versions = 10,
        .applied_versions = 3,
        .latest_expected_version = 14,
        .latest_applied_version = 3,
        .has_failed_migrations = false,
        .lock_available = false,
        .has_newer_schema_version = false,
    }, true));
}

test "integration: startup with pending migrations proceeds when enabled and lock available" {
    const decision = try decideServeMigrationPolicy(.{
        .expected_versions = 10,
        .applied_versions = 3,
        .latest_expected_version = 14,
        .latest_applied_version = 3,
        .has_failed_migrations = false,
        .lock_available = true,
        .has_newer_schema_version = false,
    }, true);
    try std.testing.expectEqual(.run_required, decision);
}

test "canonical schema bootstrap: last version is 26" {
    const migrations = canonicalMigrations();
    try std.testing.expectEqual(@as(i32, 26), migrations[migrations.len - 1].version);
}

test "every migration SQL is parseable by SqlStatementSplitter" {
    const SqlSplitter = @import("../db/sql_splitter.zig").SqlStatementSplitter;
    const migrations = canonicalMigrations();
    for (migrations) |migration| {
        const stmt_count = SqlSplitter.count(migration.sql);
        // Every migration must produce at least one statement (even version markers have SELECT 1).
        if (stmt_count == 0) {
            std.debug.print("\nFAIL: migration v{d} produces zero statements\n", .{migration.version});
            return error.EmptyMigration;
        }
    }
}
