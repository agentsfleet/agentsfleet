//! Single source of truth for schema migrations — the SQL files AND their
//! version numbers live here. `common.zig` comptime-converts `migrations` into
//! `db.Migration` items, so adding a migration is ONE edit (one line here),
//! not two files.

pub const MigrationEntry = struct { version: i32, sql: []const u8 };

pub const migrations = [_]MigrationEntry{
    .{ .version = 1, .sql = @embedFile("001_core_foundation.sql") },
    .{ .version = 2, .sql = @embedFile("002_vault_schema.sql") },
    // model_caps before platform_provider_defaults: the latter's inline FK references it.
    .{ .version = 3, .sql = @embedFile("003_model_library.sql") },
    .{ .version = 4, .sql = @embedFile("004_platform_provider_defaults.sql") },
    .{ .version = 5, .sql = @embedFile("005_core_fleets.sql") },
    .{ .version = 6, .sql = @embedFile("006_core_fleet_sessions.sql") },
    .{ .version = 7, .sql = @embedFile("007_core_fleet_approval_gates.sql") },
    .{ .version = 8, .sql = @embedFile("008_core_integration_grants.sql") },
    .{ .version = 9, .sql = @embedFile("009_core_fleet_keys.sql") },
    .{ .version = 10, .sql = @embedFile("010_memory_entries.sql") },
    .{ .version = 11, .sql = @embedFile("011_fleet_execution_telemetry.sql") },
    .{ .version = 12, .sql = @embedFile("012_api_keys.sql") },
    .{ .version = 13, .sql = @embedFile("013_core_users.sql") },
    .{ .version = 14, .sql = @embedFile("014_tenant_billing.sql") },
    .{ .version = 15, .sql = @embedFile("015_fleet_events.sql") },
    .{ .version = 16, .sql = @embedFile("016_tenant_model_selection.sql") },
    .{ .version = 17, .sql = @embedFile("017_fleet_runners.sql") },
    .{ .version = 18, .sql = @embedFile("018_fleet_runner_leases.sql") },
    .{ .version = 19, .sql = @embedFile("019_fleet_runner_affinity.sql") },
    .{ .version = 20, .sql = @embedFile("020_fleet_metering_periods.sql") },
    .{ .version = 21, .sql = @embedFile("021_fleet_runner_events.sql") },
    .{ .version = 22, .sql = @embedFile("022_account_purge_gate_bypass.sql") },
    .{ .version = 23, .sql = @embedFile("023_fleet_library.sql") },
    .{ .version = 24, .sql = @embedFile("024_core_tenant_fleet_library.sql") },
    .{ .version = 25, .sql = @embedFile("025_core_connector_installs.sql") },
    .{ .version = 26, .sql = @embedFile("026_core_connector_channels.sql") },
    .{ .version = 27, .sql = @embedFile("027_core_tenant_model_entries.sql") },
    .{ .version = 28, .sql = @embedFile("028_core_user_preferences.sql") },
    .{ .version = 29, .sql = @embedFile("029_core_fleet_schedules.sql") },
    .{ .version = 30, .sql = @embedFile("030_fleet_activity_counters.sql") },
    .{ .version = 31, .sql = @embedFile("031_fleet_runners_delete_grant.sql") },
    .{ .version = 32, .sql = @embedFile("032_fleet_events_failure_detail.sql") },
    .{ .version = 33, .sql = @embedFile("033_hot_path_indexes.sql") },
};
