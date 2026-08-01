//! What the canonical migration list must satisfy, and the proofs of it.
//!
//! Split out of `cmd/common.zig` per RULE FLL: every constant and helper here is
//! test-only, so keeping them beside the serve-time migration guard grew that
//! file to its length bound without any production caller benefiting. Nothing in
//! this module widens a production surface — it reads `canonicalMigrations()`,
//! which is public already.

const std = @import("std");
const db = @import("../db/pool.zig");
const cmd_common = @import("common.zig");
const schema_mod = @import("schema");
const schema_migrations = schema_mod.migrations;

/// `runMigrations` applies the array in order and records each version. The list
/// starts at the first slot and must increase strictly; a gap is fine, while
/// repeats or reordering could skip or re-apply schema work. Since the M154
/// rebuild the version IS the slot number (`schema/embed.zig`): first slot 100.
const FIRST_MIGRATION_VERSION: i32 = 100;

/// Slots `001`-`046` retired wholesale in the M154 rebuild and no new slot may
/// reuse one. A range replaces the single-slot guard that
/// held removed `035_workspace_create_idempotency` absent: it covers all 46.
const LAST_RETIRED_SLOT_VERSION: i32 = 46;

/// The two migrations whose presence the connector-install and channel surfaces
/// depend on. Named because a bare `550`/`560` in an assertion reads as an index.
const V_CONNECTOR_INSTALLS: i32 = 550;
const V_CHANNEL_TABLES: i32 = 560;

fn versionsIncreaseFromFirst(migrations: []const db.Migration) bool {
    for (migrations, 0..) |m, i| {
        if (i == 0) {
            if (m.version != FIRST_MIGRATION_VERSION) return false;
            continue;
        }
        if (m.version <= migrations[i - 1].version) return false;
    }
    return true;
}

test "canonical migrations: connector install + channel tables registered" {
    const migrations = cmd_common.canonicalMigrations();
    try std.testing.expectEqual(schema_migrations.len, migrations.len);
    var has_installs = false;
    var has_channels = false;
    for (migrations) |m| {
        if (m.version == V_CONNECTOR_INSTALLS) has_installs = true;
        if (m.version == V_CHANNEL_TABLES) has_channels = true;
    }
    try std.testing.expect(has_installs);
    try std.testing.expect(has_channels);
}

test "canonical migrations: versions start at the first slot and strictly increase" {
    const migrations = cmd_common.canonicalMigrations();
    try std.testing.expect(versionsIncreaseFromFirst(&migrations));
}

test "canonical migrations: an intentional gap is accepted but a duplicate is rejected" {
    const gapped = [_]db.Migration{
        .{ .version = FIRST_MIGRATION_VERSION, .sql = "" },
        .{ .version = FIRST_MIGRATION_VERSION + 20, .sql = "" },
    };
    try std.testing.expect(versionsIncreaseFromFirst(&gapped));

    const duplicated = [_]db.Migration{
        .{ .version = FIRST_MIGRATION_VERSION, .sql = "" },
        .{ .version = FIRST_MIGRATION_VERSION, .sql = "" },
    };
    try std.testing.expect(!versionsIncreaseFromFirst(&duplicated));
}

test "canonical migrations: a list not starting at the first slot, or running backwards, is rejected" {
    // An off-by-one start or reversed pair would make schema history ambiguous.
    const wrong_start = [_]db.Migration{
        .{ .version = FIRST_MIGRATION_VERSION - 1, .sql = "" },
        .{ .version = FIRST_MIGRATION_VERSION, .sql = "" },
    };
    try std.testing.expect(!versionsIncreaseFromFirst(&wrong_start));

    const descending = [_]db.Migration{
        .{ .version = FIRST_MIGRATION_VERSION + 10, .sql = "" },
        .{ .version = FIRST_MIGRATION_VERSION, .sql = "" },
    };
    try std.testing.expect(!versionsIncreaseFromFirst(&descending));

    // Vacuously ordered: the embedded list is asserted non-empty separately, so an
    // empty slice never reaches a caller that would misread `true` as "migrations ran".
    const empty: []const db.Migration = &.{};
    try std.testing.expect(versionsIncreaseFromFirst(empty));
}

test "canonical schema bootstrap: every slot sits in the new numbering space" {
    const migrations = cmd_common.canonicalMigrations();
    try std.testing.expect(migrations.len > 0);
    // Ties "starts at 100" to "never reuses 001-046" so the two cannot drift.
    try std.testing.expect(FIRST_MIGRATION_VERSION > LAST_RETIRED_SLOT_VERSION);
    for (migrations) |migration| {
        // The floor is the FIRST SLOT, not merely the retired ceiling. A slot
        // numbered 47-99 clears the retired range while still sitting outside
        // the layer scheme entirely — and that is not hypothetical: a pending
        // spec is written against `schema/047_repair_proposals.sql`, from before
        // this renumbering. Asserting the floor refuses it at the slot
        // rather than later, at a confusing ordering failure.
        std.testing.expect(migration.version >= FIRST_MIGRATION_VERSION) catch |err| {
            std.debug.print(
                "\nFAIL: slot v{d} is below the first slot ({d}) — renumber it into a layer\n",
                .{ migration.version, FIRST_MIGRATION_VERSION },
            );
            return err;
        };
    }
}
