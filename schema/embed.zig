//! Single source of truth for schema migrations — the SQL files AND their
//! version numbers live here. `common.zig` comptime-converts `migrations` into
//! `db.Migration` items, so adding a migration is ONE edit (one line here),
//! not two files.
//!
//! **The version IS the slot number.** `550_connector_installs.sql` applies as
//! version 550. The two were independent counters before the rebuild, which is
//! how a named slot-version constant could drift from the file it named; tying
//! them makes that class of drift unrepresentable (RULE MIG).
//!
//! Slots are ordered by dependency layer, not by chronology, and the layer is the
//! leading digit — `1xx` substrate, `2xx` identity, `3xx` secrets, `4xx`
//! catalogue, `5xx` fleets, `6xx` runner control plane, `7xx` money, `8xx`
//! history. That is the order a database bootstrapped from empty must create them
//! in, so reading top to bottom reads the dependency graph. Gaps of 10 leave room
//! to insert without renumbering the world.
//!
//! Numbering starts at `1xx` because slots `001`–`046` are retired wholesale by
//! the rebuild: no new slot can reuse a retired number, so "is this slot retired?"
//! stays a one-glob assertion rather than a judgement call.

pub const MigrationEntry = struct { version: i32, sql: []const u8 };

pub const migrations = [_]MigrationEntry{
    // ── 1xx substrate — runs before any table exists ────────────────────────
    .{ .version = 100, .sql = @embedFile("100_schemas.sql") },
    .{ .version = 110, .sql = @embedFile("110_roles_and_privileges.sql") },
    .{ .version = 120, .sql = @embedFile("120_metering_role.sql") },

    // ── 2xx identity — the tenant root and everything that authenticates ────
    .{ .version = 200, .sql = @embedFile("200_tenants.sql") },
    .{ .version = 210, .sql = @embedFile("210_workspaces.sql") },
    .{ .version = 220, .sql = @embedFile("220_users.sql") },
    .{ .version = 230, .sql = @embedFile("230_memberships.sql") },
    .{ .version = 240, .sql = @embedFile("240_api_keys.sql") },

    // ── 3xx secrets ────────────────────────────────────────────────────────
    .{ .version = 300, .sql = @embedFile("300_vault_secrets.sql") },

    // ── 4xx catalogue — model_library first: 420's inline FK references it ──
    .{ .version = 400, .sql = @embedFile("400_model_library.sql") },
    .{ .version = 410, .sql = @embedFile("410_model_catalogue_revision.sql") },
    .{ .version = 420, .sql = @embedFile("420_platform_provider_defaults.sql") },
    .{ .version = 430, .sql = @embedFile("430_tenant_model_selection.sql") },
    .{ .version = 440, .sql = @embedFile("440_tenant_model_entries.sql") },
    .{ .version = 450, .sql = @embedFile("450_fleet_library.sql") },
    .{ .version = 460, .sql = @embedFile("460_tenant_fleet_library.sql") },

    // ── 5xx fleets — 500 is the parent almost every 5xx/6xx/8xx row cascades from
    .{ .version = 500, .sql = @embedFile("500_fleets.sql") },
    .{ .version = 510, .sql = @embedFile("510_fleet_sessions.sql") },
    .{ .version = 520, .sql = @embedFile("520_fleet_schedules.sql") },
    .{ .version = 540, .sql = @embedFile("540_integration_grants.sql") },
    .{ .version = 550, .sql = @embedFile("550_connector_installs.sql") },
    .{ .version = 560, .sql = @embedFile("560_connector_channels.sql") },
    .{ .version = 570, .sql = @embedFile("570_user_preferences.sql") },

    // ── 6xx runner control plane — indexes follow the table they sit on ─────
    .{ .version = 600, .sql = @embedFile("600_runners.sql") },
    .{ .version = 610, .sql = @embedFile("610_runner_leases.sql") },
    .{ .version = 620, .sql = @embedFile("620_runner_lease_indexes.sql") },
    .{ .version = 630, .sql = @embedFile("630_runner_affinity.sql") },
    .{ .version = 640, .sql = @embedFile("640_runner_events.sql") },
    .{ .version = 650, .sql = @embedFile("650_runner_lifetime_counters.sql") },

    // ── 7xx money ──────────────────────────────────────────────────────────
    .{ .version = 700, .sql = @embedFile("700_tenant_wallet.sql") },
    .{ .version = 710, .sql = @embedFile("710_usage_ledger.sql") },
    .{ .version = 720, .sql = @embedFile("720_usage_ledger_indexes.sql") },

    // ── 8xx history — 890 last: its triggers attach to 800 and 880 ──────────
    .{ .version = 800, .sql = @embedFile("800_fleet_events.sql") },
    .{ .version = 810, .sql = @embedFile("810_fleet_approval_gates.sql") },
    .{ .version = 820, .sql = @embedFile("820_memory_entries.sql") },
    .{ .version = 880, .sql = @embedFile("880_fleet_activity_counters.sql") },
    .{ .version = 890, .sql = @embedFile("890_fleet_activity_counter_triggers.sql") },
};
