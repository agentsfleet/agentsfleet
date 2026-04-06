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

pub fn canonicalMigrations() [17]db.Migration {
    const schema = @import("schema");
    return .{
        .{ .version = 1, .sql = schema.core_foundation_sql },
        .{ .version = 2, .sql = schema.core_workflow_sql },
        .{ .version = 3, .sql = schema.core_results_events_sql },
        .{ .version = 4, .sql = schema.vault_sql },
        .{ .version = 6, .sql = schema.side_effect_ledger_sql },
        .{ .version = 7, .sql = schema.side_effect_outbox_sql },
        .{ .version = 8, .sql = schema.harness_control_plane_sql },
        .{ .version = 9, .sql = schema.rls_tenant_isolation_sql },
        .{ .version = 11, .sql = schema.profile_linkage_audit_sql },
        .{ .version = 14, .sql = schema.workspace_entitlements_sql },
        .{ .version = 15, .sql = schema.usage_metering_billing_sql },
        .{ .version = 16, .sql = schema.workspace_billing_state_sql },
        .{ .version = 17, .sql = schema.workspace_free_credit_sql },
        .{ .version = 18, .sql = schema.agent_scoring_baseline_sql },
        .{ .version = 19, .sql = schema.agent_score_persistence_api_sql },
        .{ .version = 20, .sql = schema.agent_failure_analysis_context_sql },
        .{ .version = 21, .sql = schema.platform_llm_keys_sql },
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

test "canonical schema bootstrap includes scoring config in base schema" {
    const migrations = canonicalMigrations();
    try std.testing.expectEqual(@as(i32, 21), migrations[migrations.len - 1].version);
    try std.testing.expect(std.mem.containsAtLeast(u8, migrations[6].sql, 1, "trust_streak_runs INTEGER NOT NULL"));
    try std.testing.expect(std.mem.containsAtLeast(u8, migrations[6].sql, 1, "trust_level   TEXT NOT NULL"));
    try std.testing.expect(!std.mem.containsAtLeast(u8, migrations[6].sql, 1, "trust_streak_runs INTEGER NOT NULL DEFAULT 0"));
    try std.testing.expect(!std.mem.containsAtLeast(u8, migrations[6].sql, 1, "trust_level   TEXT NOT NULL DEFAULT 'UNEARNED'"));
    try std.testing.expect(std.mem.containsAtLeast(u8, migrations[6].sql, 1, "CREATE TABLE agent_improvement_proposals"));
    try std.testing.expect(std.mem.containsAtLeast(u8, migrations[6].sql, 1, "generation_status   TEXT NOT NULL"));
    try std.testing.expect(!std.mem.containsAtLeast(u8, migrations[6].sql, 1, "CHECK (trigger_reason IN"));
    try std.testing.expect(!std.mem.containsAtLeast(u8, migrations[6].sql, 1, "CHECK (approval_mode IN"));
    try std.testing.expect(!std.mem.containsAtLeast(u8, migrations[6].sql, 1, "CHECK (generation_status IN"));
    try std.testing.expect(!std.mem.containsAtLeast(u8, migrations[6].sql, 1, "CHECK (status IN"));
    try std.testing.expect(!std.mem.containsAtLeast(u8, migrations[6].sql, 1, "WHERE status ="));
    try std.testing.expect(std.mem.containsAtLeast(u8, migrations[6].sql, 1, "CREATE TABLE harness_change_log"));
    try std.testing.expect(std.mem.containsAtLeast(u8, migrations[9].sql, 1, "enable_agent_scoring BOOLEAN NOT NULL"));
    try std.testing.expect(std.mem.containsAtLeast(u8, migrations[9].sql, 1, "agent_scoring_weights_json TEXT NOT NULL"));
    try std.testing.expect(std.mem.containsAtLeast(u8, migrations[13].sql, 1, "CREATE TABLE workspace_latency_baseline"));
    try std.testing.expect(std.mem.containsAtLeast(u8, migrations[14].sql, 1, "CREATE TABLE agent_run_scores"));
    try std.testing.expect(std.mem.containsAtLeast(u8, migrations[15].sql, 1, "CREATE TABLE agent_run_analysis"));
    try std.testing.expect(!std.mem.containsAtLeast(u8, migrations[14].sql, 1, "tier             TEXT"));
}
