//! Unit proof of the privilege boundary, asserted against the embedded slot
//! text — where a superuser test connection cannot hide a widened grant.
//!
//! The integration tier proves what a live PostgreSQL enforces
//! (`schema_privilege_integration_test.zig`); this tier pins the DECLARED
//! posture: which grants exist in the slots, that every elevation membership
//! is dormant, and that the composite metering role reaches exactly the
//! fenced statement's tables. A change that re-widens `api_runtime` fails
//! here at `zig build test` speed, with the slot text in hand.

const std = @import("std");
const testing = std.testing;
const schema = @import("schema");
const pool_elevation = @import("pool_elevation.zig");

/// Every embedded migration, so a grant hidden in an unexpected slot is still
/// seen — the scan is over the whole schema, not a curated allowlist.
fn allSlots() []const schema.MigrationEntry {
    return &schema.migrations;
}

/// Iterate `sql` line by line, skipping `--` comment lines, calling `visit`
/// with each statement-bearing line. Grants in these slots are one-per-line
/// by convention (SCHEMA_CONVENTIONS), which is what makes a line scan sound.
fn eachGrantLine(sql: []const u8, ctx: anytype, comptime visit: fn (@TypeOf(ctx), []const u8) anyerror!void) !void {
    var lines = std.mem.splitScalar(u8, sql, '\n');
    while (lines.next()) |raw| {
        const line = std.mem.trim(u8, raw, " \t\r");
        if (std.mem.startsWith(u8, line, "--")) continue;
        if (!std.mem.startsWith(u8, line, "GRANT ")) continue;
        try visit(ctx, line);
    }
}

test "every elevation membership granted to api_runtime is dormant until SET ROLE" {
    // Dimension 1.4. A bare `GRANT <role> TO api_runtime` follows api_runtime's
    // INHERIT attribute (CREATE ROLE defaults it TRUE): the privileges would
    // apply ambiently and nothing would ever elevate. `WITH INHERIT FALSE,
    // SET TRUE` is what makes membership dormant. The count is asserted so a
    // scan that matches nothing cannot pass vacuously.
    const Counter = struct { memberships: *usize };
    var memberships: usize = 0;
    for (allSlots()) |slot| {
        try eachGrantLine(slot.sql, Counter{ .memberships = &memberships }, struct {
            fn visit(c: Counter, line: []const u8) !void {
                // Membership grants have no ` ON ` clause; privilege grants do.
                if (std.mem.indexOf(u8, line, " ON ") != null) return;
                if (std.mem.indexOf(u8, line, " TO api_runtime") == null) return;
                c.memberships.* += 1;
                try testing.expect(std.mem.indexOf(u8, line, "WITH INHERIT FALSE, SET TRUE") != null);
            }
        }.visit);
    }
    // memory_runtime + vault_runtime + billing_runtime (schema/110) and
    // metering_runtime (schema/120).
    try testing.expectEqual(@as(usize, 4), memberships);
}

test "api_runtime holds no direct grant on the secret store or the wallet" {
    // Dimension 1.1's declared-posture twin (the catalogue form runs in the
    // integration tier). Any GRANT naming both a money/secret table and
    // api_runtime is the regression this exists to catch.
    const Ctx = struct {};
    for (allSlots()) |slot| {
        try eachGrantLine(slot.sql, Ctx{}, struct {
            fn visit(_: Ctx, line: []const u8) !void {
                const names_api = std.mem.indexOf(u8, line, " TO api_runtime") != null;
                if (!names_api) return;
                try testing.expect(std.mem.indexOf(u8, line, "vault.secrets") == null);
                try testing.expect(std.mem.indexOf(u8, line, "billing.tenant_wallet") == null);
                // The ledger is the one deliberate exception: SELECT only.
                if (std.mem.indexOf(u8, line, "billing.usage_ledger") != null) {
                    try testing.expect(std.mem.startsWith(u8, line, "GRANT SELECT ON"));
                }
            }
        }.visit);
    }
}

test "metering_runtime's direct grants match the fenced statement's footprint exactly" {
    // Dimension 1.5. Reach stays enumerable: the grant list IS the statement's
    // table list — three `fleet` tables, nothing else, and the money tables
    // arrive only through the inheriting billing membership.
    const Tally = struct {
        fleet_tables: *usize,
        saw_usage: *bool,
        saw_billing_membership: *bool,
        saw_api_membership: *bool,
    };
    var fleet_tables: usize = 0;
    var saw_usage = false;
    var saw_billing_membership = false;
    var saw_api_membership = false;
    for (allSlots()) |slot| {
        try eachGrantLine(slot.sql, Tally{
            .fleet_tables = &fleet_tables,
            .saw_usage = &saw_usage,
            .saw_billing_membership = &saw_billing_membership,
            .saw_api_membership = &saw_api_membership,
        }, struct {
            fn visit(t: Tally, line: []const u8) !void {
                if (std.mem.indexOf(u8, line, "TO metering_runtime") != null) {
                    // Direct object grants: only the three fleet tables + schema USAGE.
                    if (std.mem.indexOf(u8, line, " ON fleet.") != null) {
                        t.fleet_tables.* += 1;
                        const allowed = std.mem.indexOf(u8, line, "fleet.runner_leases") != null or
                            std.mem.indexOf(u8, line, "fleet.runner_affinity") != null or
                            std.mem.indexOf(u8, line, "fleet.runner_lifetime_counters") != null;
                        try testing.expect(allowed);
                    } else if (std.mem.indexOf(u8, line, "USAGE ON SCHEMA fleet") != null) {
                        t.saw_usage.* = true;
                    } else if (std.mem.indexOf(u8, line, "GRANT billing_runtime") != null) {
                        // The composite's one membership — INHERITING on purpose.
                        try testing.expect(std.mem.indexOf(u8, line, "WITH INHERIT TRUE") != null);
                        t.saw_billing_membership.* = true;
                    } else {
                        // No other object may be granted to the composite.
                        try testing.expect(false);
                    }
                    // Never a direct grant on the money tables.
                    try testing.expect(std.mem.indexOf(u8, line, "tenant_wallet") == null);
                    try testing.expect(std.mem.indexOf(u8, line, "usage_ledger") == null);
                    try testing.expect(std.mem.indexOf(u8, line, "vault.secrets") == null);
                }
                if (std.mem.indexOf(u8, line, "GRANT metering_runtime TO api_runtime") != null) {
                    t.saw_api_membership.* = true;
                }
            }
        }.visit);
    }
    try testing.expectEqual(@as(usize, 3), fleet_tables);
    try testing.expect(saw_usage);
    try testing.expect(saw_billing_membership);
    try testing.expect(saw_api_membership);
}

test "the elevation module's role names appear verbatim in the slots that create them" {
    // RULE UFS both directions: the Zig constants and the SQL identifiers are
    // one vocabulary. A rename on either side fails here, not at runtime.
    var found_vault = false;
    var found_billing = false;
    var found_metering = false;
    var found_memory = false;
    for (allSlots()) |slot| {
        if (slot.version != 110 and slot.version != 120) continue;
        if (std.mem.indexOf(u8, slot.sql, pool_elevation.ROLE_NAME_VAULT) != null) found_vault = true;
        if (std.mem.indexOf(u8, slot.sql, pool_elevation.ROLE_NAME_BILLING) != null) found_billing = true;
        if (std.mem.indexOf(u8, slot.sql, pool_elevation.ROLE_NAME_METERING) != null) found_metering = true;
        if (std.mem.indexOf(u8, slot.sql, pool_elevation.ROLE_NAME_MEMORY) != null) found_memory = true;
    }
    try testing.expect(found_vault);
    try testing.expect(found_billing);
    try testing.expect(found_metering);
    try testing.expect(found_memory);
}

test "slot 120 exists, runs after 110, and before every table slot" {
    // The composite role inherits billing_runtime (declared in 110) and is
    // named by grants in the 6xx slots — order is a bootstrap correctness
    // property, pinned here rather than trusted to the array's prose.
    var saw_110 = false;
    var saw_120_at: ?usize = null;
    for (allSlots(), 0..) |slot, i| {
        if (slot.version == 110) {
            try testing.expect(saw_120_at == null);
            saw_110 = true;
        }
        if (slot.version == 120) {
            try testing.expect(saw_110);
            saw_120_at = i;
        }
        if (slot.version >= 200) {
            try testing.expect(saw_120_at != null);
            break;
        }
    }
}
