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

/// The grantee list of a GRANT line: everything after the last ` TO `, with a
/// trailing `WITH ...` clause and the semicolon removed.
fn granteesOf(line: []const u8) []const u8 {
    const to = std.mem.lastIndexOf(u8, line, " TO ") orelse return "";
    var rest = line[to + " TO ".len ..];
    if (std.mem.indexOf(u8, rest, " WITH ")) |with| rest = rest[0..with];
    return std.mem.trim(u8, rest, " ;");
}

/// Whether `line` grants to `role`, parsed as a comma-separated grantee list.
/// A substring match cannot answer this: PostgreSQL takes several grantees per
/// statement, and the slots already use that spelling (`TO ops_readonly_human,
/// ops_readonly_fleet`), so `GRANT ... TO reporting, api_runtime` would slip
/// past a naive ` TO api_runtime` scan.
fn grantsTo(line: []const u8, role: []const u8) bool {
    var it = std.mem.splitScalar(u8, granteesOf(line), ',');
    while (it.next()) |grantee| {
        if (std.mem.eql(u8, std.mem.trim(u8, grantee, " ;"), role)) return true;
    }
    return false;
}

test "the grantee parser reads lists, not substrings" {
    // The scanner's own regression guard: every assertion below is a spelling
    // that a substring match gets wrong in one direction or the other.
    try testing.expect(grantsTo("GRANT SELECT ON x TO reporting, api_runtime;", "api_runtime"));
    try testing.expect(grantsTo("GRANT SELECT ON x TO api_runtime, reporting;", "api_runtime"));
    try testing.expect(grantsTo("GRANT memory_runtime TO api_runtime WITH INHERIT FALSE, SET TRUE;", "api_runtime"));
    // `api_runtime_readonly` is a different role and must not match.
    try testing.expect(!grantsTo("GRANT SELECT ON x TO api_runtime_readonly;", "api_runtime"));
    try testing.expect(!grantsTo("GRANT SELECT ON x TO billing_runtime;", "api_runtime"));
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
                if (!grantsTo(line, "api_runtime")) return;
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
    const Tally = struct { api_grants: *usize, ledger_exception: *usize };
    var api_grants: usize = 0;
    var ledger_exception: usize = 0;
    for (allSlots()) |slot| {
        try eachGrantLine(slot.sql, Tally{
            .api_grants = &api_grants,
            .ledger_exception = &ledger_exception,
        }, struct {
            fn visit(t: Tally, line: []const u8) !void {
                if (!grantsTo(line, "api_runtime")) return;
                t.api_grants.* += 1;
                try testing.expect(std.mem.indexOf(u8, line, "vault.secrets") == null);
                try testing.expect(std.mem.indexOf(u8, line, "billing.tenant_wallet") == null);
                // The ledger is the one deliberate exception: SELECT only.
                if (std.mem.indexOf(u8, line, "billing.usage_ledger") != null) {
                    try testing.expect(std.mem.startsWith(u8, line, "GRANT SELECT ON"));
                    t.ledger_exception.* += 1;
                }
            }
        }.visit);
    }
    // Positive control: without it this test passes just as green when the
    // scanner stops matching anything at all (a changed grant spelling, a
    // renamed role, a slot dropped from the embed list).
    try testing.expect(api_grants > 0);
    try testing.expectEqual(@as(usize, 1), ledger_exception);
}

test "metering_runtime's direct grants match the fenced statement's footprint exactly" {
    // Dimension 1.5. Reach stays enumerable: the grant list IS the statement's
    // table list — the three `fleet` tables it writes, plus the wallet and the
    // ledger with only the verbs it issues. Nothing arrives by membership,
    // which is the property that makes "enumerable" literally true: an
    // inheriting membership in billing_runtime would silently re-add INSERT
    // and DELETE on the wallet, neither of which the statement issues.
    const Tally = struct {
        objects: *usize,
        usage_schemas: *usize,
        saw_api_membership: *bool,
    };
    var objects: usize = 0;
    var usage_schemas: usize = 0;
    var saw_api_membership = false;
    for (allSlots()) |slot| {
        try eachGrantLine(slot.sql, Tally{
            .objects = &objects,
            .usage_schemas = &usage_schemas,
            .saw_api_membership = &saw_api_membership,
        }, struct {
            fn visit(t: Tally, line: []const u8) !void {
                if (grantsTo(line, "metering_runtime")) {
                    if (std.mem.indexOf(u8, line, " ON fleet.") != null) {
                        t.objects.* += 1;
                        const allowed = std.mem.indexOf(u8, line, "fleet.runner_leases") != null or
                            std.mem.indexOf(u8, line, "fleet.runner_affinity") != null or
                            std.mem.indexOf(u8, line, "fleet.runner_lifetime_counters") != null;
                        try testing.expect(allowed);
                    } else if (std.mem.indexOf(u8, line, " ON billing.tenant_wallet") != null) {
                        t.objects.* += 1;
                        // Reads the balance, updates it. Never creates a wallet
                        // (the starter grant does) and never deletes one (the
                        // tenant cascade does).
                        try testing.expect(std.mem.startsWith(u8, line, "GRANT SELECT, UPDATE ON"));
                    } else if (std.mem.indexOf(u8, line, " ON billing.usage_ledger") != null) {
                        t.objects.* += 1;
                        try testing.expect(std.mem.startsWith(u8, line, "GRANT SELECT, INSERT, UPDATE ON"));
                    } else if (std.mem.indexOf(u8, line, "USAGE ON SCHEMA") != null) {
                        t.usage_schemas.* += 1;
                    } else {
                        // No other object, and no membership, may be granted to
                        // the composite — an ` ON `-less line here is a role
                        // membership sneaking privilege in sideways.
                        try testing.expect(false);
                    }
                    // The secret store is never in the metering footprint.
                    try testing.expect(std.mem.indexOf(u8, line, "vault.secrets") == null);
                }
                if (std.mem.indexOf(u8, line, "GRANT metering_runtime TO api_runtime") != null) {
                    t.saw_api_membership.* = true;
                }
            }
        }.visit);
    }
    // Three fleet tables + the wallet + the ledger.
    try testing.expectEqual(@as(usize, 5), objects);
    // One `GRANT USAGE ON SCHEMA fleet, billing` line.
    try testing.expectEqual(@as(usize, 1), usage_schemas);
    try testing.expect(saw_api_membership);
}

test "no elevation role inherits another role's privileges" {
    // The composite was the only membership between elevation roles, and it is
    // gone: every role's reach is now exactly its own direct grants. A future
    // `GRANT <role> TO <role>` re-opens the sideways path this milestone
    // closed, so it fails here rather than widening a boundary quietly.
    const elevation_roles = [_][]const u8{ "vault_runtime", "billing_runtime", "metering_runtime", "memory_runtime" };
    const Ctx = struct { roles: []const []const u8 };
    for (allSlots()) |slot| {
        try eachGrantLine(slot.sql, Ctx{ .roles = &elevation_roles }, struct {
            fn visit(c: Ctx, line: []const u8) !void {
                // Object grants carry ` ON `; what is left is a membership.
                if (std.mem.indexOf(u8, line, " ON ") != null) return;
                for (c.roles) |role| {
                    if (grantsTo(line, role)) {
                        std.debug.print("\nFAIL: elevation role {s} is granted a membership: {s}\n", .{ role, line });
                        return error.TestUnexpectedResult;
                    }
                }
            }
        }.visit);
    }
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
