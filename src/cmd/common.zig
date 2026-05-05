const db = @import("../db/pool.zig");
const std = @import("std");
const log = std.log.scoped(.zombied);

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

pub fn canonicalMigrations() [19]db.Migration {
    const schema = @import("schema");
    return .{
        .{ .version = 1, .sql = schema.core_foundation_sql },
        .{ .version = 2, .sql = schema.vault_sql },
        .{ .version = 4, .sql = schema.workspace_entitlements_sql },
        .{ .version = 5, .sql = schema.agent_failure_analysis_context_sql },
        .{ .version = 6, .sql = schema.platform_llm_keys_sql },
        .{ .version = 7, .sql = schema.core_zombies_sql },
        .{ .version = 8, .sql = schema.core_zombie_sessions_sql },
        .{ .version = 9, .sql = schema.core_zombie_approval_gates_sql },
        .{ .version = 10, .sql = schema.core_integration_grants_sql },
        .{ .version = 11, .sql = schema.core_agent_keys_sql },
        .{ .version = 12, .sql = schema.workspace_integrations_sql },
        .{ .version = 13, .sql = schema.memory_entries_sql },
        .{ .version = 14, .sql = schema.zombie_execution_telemetry_sql },
        .{ .version = 15, .sql = schema.api_keys_sql },
        .{ .version = 16, .sql = schema.core_users_sql },
        .{ .version = 17, .sql = schema.tenant_billing_sql },
        .{ .version = 18, .sql = schema.zombie_events_sql },
        .{ .version = 19, .sql = schema.model_caps_sql },
        .{ .version = 20, .sql = schema.tenant_providers_sql },
    };
}

pub fn migrateOnStartEnabledFromEnv(alloc: std.mem.Allocator) MigrationGuardError!bool {
    const raw = std.process.getEnvVarOwned(alloc, "MIGRATE_ON_START") catch return false;
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

pub fn enforceServeMigrationSafety(
    pool: *db.Pool,
    migrate_on_start: bool,
) (MigrationGuardError || anyerror)!void {
    const migrations = canonicalMigrations();
    const state = try db.inspectMigrationState(pool, &migrations);
    const decision = try decideServeMigrationPolicy(state, migrate_on_start);

    switch (decision) {
        .allow_without_running => return,
        .run_required => {
            log.warn("startup.migration_auto_apply status=start reason=MIGRATE_ON_START enabled", .{});
            try db.runMigrations(pool, &migrations);

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

test "migrateOnStartEnabledFromEnv parses known values" {
    const alloc = std.testing.allocator;

    try std.testing.expect(!try migrateOnStartEnabledFromEnv(alloc));

    try std.posix.setenv("MIGRATE_ON_START", "1", true);
    try std.testing.expect(try migrateOnStartEnabledFromEnv(alloc));

    try std.posix.setenv("MIGRATE_ON_START", "false", true);
    try std.testing.expect(!try migrateOnStartEnabledFromEnv(alloc));

    try std.posix.setenv("MIGRATE_ON_START", "bad", true);
    try std.testing.expectError(MigrationGuardError.InvalidMigrateOnStart, migrateOnStartEnabledFromEnv(alloc));
    try std.posix.unsetenv("MIGRATE_ON_START");
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

test "canonical schema bootstrap: last version is 20 and entitlements carry scoring config" {
    const migrations = canonicalMigrations();
    try std.testing.expectEqual(@as(i32, 20), migrations[migrations.len - 1].version);

    var entitlements_sql: ?[]const u8 = null;
    for (migrations) |m| {
        if (m.version == 4) entitlements_sql = m.sql;
    }
    const ent = entitlements_sql orelse return error.TestExpectedEqual;
    try std.testing.expect(std.mem.containsAtLeast(u8, ent, 1, "enable_agent_scoring BOOLEAN NOT NULL"));
    try std.testing.expect(std.mem.containsAtLeast(u8, ent, 1, "agent_scoring_weights_json TEXT NOT NULL"));
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
