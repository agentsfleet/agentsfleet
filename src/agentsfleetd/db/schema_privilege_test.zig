//! Unit proof that the privilege boundary the roles describe is the boundary the
//! schema actually builds.
//!
//! These assertions run against the EMBEDDED slot text rather than a live
//! database, on purpose. Local Docker and Continuous Integration both connect as
//! a superuser (`docker-compose.yml`), for whom every privilege check passes
//! trivially — so a live-role test skips or silently lies exactly where this
//! boundary most needs proving. That gap is not hypothetical: the `memory` role
//! split shipped with the same latent defect these tests now catch, and its
//! integration test passed throughout, because a superuser is never refused.
//!
//! The defect being guarded: `GRANT <role> TO api_runtime` with no INHERIT
//! clause takes its inheritance from api_runtime's own INHERIT attribute, which
//! CREATE ROLE defaults to TRUE. The member role's privileges then apply
//! ambiently on every connection, no handler ever has to elevate, and the
//! boundary exists only in the comment above the grant.
//!
//! Scanning is STATEMENT-based, not line-based. A grant that wraps across lines
//! — which the vault column grant does — reads identically to a single-line one,
//! so the boundary cannot be widened by reformatting.

const std = @import("std");
const schema = @import("schema");
const SqlStatementSplitter = @import("sql_splitter.zig").SqlStatementSplitter;

const R_API = "api_runtime";
const R_METERING = "metering_runtime";
const R_BILLING = "billing_runtime";
const T_VAULT_SECRETS = "vault.secrets";
const T_WALLET = "billing.tenant_wallet";
const T_LEDGER = "billing.usage_ledger";
const DORMANT_MEMBERSHIP = "WITH INHERIT FALSE, SET TRUE";
const TO_API = "TO " ++ R_API;
const TO_METERING = "TO " ++ R_METERING;

/// Every role `api_runtime` may assume. Asserted as a count so a scan that
/// matches nothing fails loudly instead of passing vacuously.
const DORMANT_MEMBERSHIPS_EXPECTED: usize = 4;

/// The fenced settle/renewal statement's `fleet` footprint — the whole reason
/// `metering_runtime` exists (schema/120). Wallet and ledger are deliberately
/// absent: those arrive by membership of `billing_runtime`, not by restatement.
const METERING_FLEET_TABLES = [_][]const u8{
    "fleet.runner_leases",
    "fleet.runner_affinity",
    "fleet.runner_lifetime_counters",
};

/// The sealed envelope. `api_runtime` may not name one of these at any privilege
/// level; reaching a secret is what elevation is for (schema/300).
const SECRET_COLUMNS = [_][]const u8{
    "ciphertext",
    "encrypted_dek",
    "dek_nonce",
    "dek_tag",
    "nonce",
    "tag",
    "kek_version",
};

/// One statement with comment lines dropped and line breaks collapsed to single
/// spaces, so a wrapped grant matches the same substrings as an unwrapped one.
fn stripped(alloc: std.mem.Allocator, stmt: []const u8) ![]u8 {
    var out: std.ArrayList(u8) = .empty;
    errdefer out.deinit(alloc);
    var lines = std.mem.splitScalar(u8, stmt, '\n');
    while (lines.next()) |raw| {
        const line = std.mem.trim(u8, raw, " \t\r");
        if (line.len == 0 or std.mem.startsWith(u8, line, "--")) continue;
        if (out.items.len > 0) try out.append(alloc, ' ');
        try out.appendSlice(alloc, line);
    }
    return out.toOwnedSlice(alloc);
}

/// A role-membership grant (`GRANT <role> TO <role>`) rather than an object
/// grant (`GRANT <privs> ON <object> TO <role>`).
fn isMembershipGrant(s: []const u8) bool {
    if (!std.mem.startsWith(u8, s, "GRANT ")) return false;
    if (std.mem.indexOf(u8, s, " ON ") != null) return false;
    return std.mem.indexOf(u8, s, " TO ") != null;
}

/// Calls `handler` with every GRANT statement in the embedded schema, stripped.
fn forEachGrant(
    ctx: anytype,
    comptime handler: fn (@TypeOf(ctx), i32, []const u8) anyerror!void,
) !void {
    const alloc = std.testing.allocator;
    for (schema.migrations) |m| {
        var splitter = SqlStatementSplitter.init(m.sql);
        while (splitter.next()) |stmt| {
            const s = try stripped(alloc, stmt);
            defer alloc.free(s);
            if (!std.mem.startsWith(u8, s, "GRANT ")) continue;
            try handler(ctx, m.version, s);
        }
    }
}

const Counter = struct { n: usize = 0 };

fn countDormantApiMembership(c: *Counter, version: i32, s: []const u8) !void {
    if (!isMembershipGrant(s)) return;
    if (std.mem.indexOf(u8, s, TO_API) == null) return;
    std.testing.expect(std.mem.indexOf(u8, s, DORMANT_MEMBERSHIP) != null) catch |err| {
        std.debug.print(
            "\nFAIL: ambient membership in slot v{d} — api_runtime holds this role's " ++
                "privileges with no SET ROLE: {s}\n",
            .{ version, s },
        );
        return err;
    };
    c.n += 1;
}

test "every role membership granted to api_runtime stays dormant until SET ROLE" {
    var counter = Counter{};
    try forEachGrant(&counter, countDormantApiMembership);
    try std.testing.expectEqual(DORMANT_MEMBERSHIPS_EXPECTED, counter.n);
}

fn refuseWalletGrant(_: void, version: i32, s: []const u8) !void {
    if (std.mem.indexOf(u8, s, TO_API) == null) return;
    std.testing.expect(std.mem.indexOf(u8, s, T_WALLET) == null) catch |err| {
        std.debug.print(
            "\nFAIL: slot v{d} hands api_runtime a direct grant on the wallet, which " ++
                "is forbidden at any privilege level: {s}\n",
            .{ version, s },
        );
        return err;
    };
}

test "api_runtime holds no grant of any kind on the wallet" {
    try forEachGrant({}, refuseWalletGrant);
}

fn refuseSecretColumn(_: void, version: i32, s: []const u8) !void {
    if (std.mem.indexOf(u8, s, T_VAULT_SECRETS) == null) return;
    if (std.mem.indexOf(u8, s, TO_API) == null) return;

    // Column-scoped or nothing: a table-wide grant would carry the envelope.
    const open = std.mem.indexOfScalar(u8, s, '(') orelse {
        std.debug.print(
            "\nFAIL: slot v{d} grants api_runtime the WHOLE vault table; only a " ++
                "column grant may name it: {s}\n",
            .{ version, s },
        );
        return error.VaultGrantNotColumnScoped;
    };
    const close = std.mem.indexOfScalar(u8, s[open..], ')') orelse return error.MalformedColumnGrant;

    var columns = std.mem.splitScalar(u8, s[open + 1 .. open + close], ',');
    while (columns.next()) |raw| {
        const column = std.mem.trim(u8, raw, " \t");
        for (SECRET_COLUMNS) |secret| {
            std.testing.expect(!std.mem.eql(u8, column, secret)) catch |err| {
                std.debug.print(
                    "\nFAIL: slot v{d} lets api_runtime read the sealed column `{s}` " ++
                        "without elevating: {s}\n",
                    .{ version, column, s },
                );
                return err;
            };
        }
    }
}

test "api_runtime cannot reach a sealed vault column" {
    // The vault grant is column-scoped rather than absent: three unelevated
    // statements span vault and core in one statement and read only non-secret
    // columns, so a table grant would be too wide and whole-table elevation too
    // coarse.
    try forEachGrant({}, refuseSecretColumn);
}

fn requireLedgerSelectOnly(_: void, _: i32, s: []const u8) !void {
    if (std.mem.indexOf(u8, s, T_LEDGER) == null) return;
    if (std.mem.indexOf(u8, s, TO_API) == null) return;
    try std.testing.expect(std.mem.startsWith(u8, s, "GRANT SELECT ON"));
}

test "the ledger stays readable without elevating" {
    // The privilege split fences money that MOVES; a charge history does not
    // move. Four
    // readers depend on this grant (charges list, events-list cost join, fleet
    // outcome reads, fleet delete) and the pre-rebuild slot omitted it, which
    // would answer all four with insufficient_privilege.
    try forEachGrant({}, requireLedgerSelectOnly);
    var seen = false;
    const alloc = std.testing.allocator;
    for (schema.migrations) |m| {
        var splitter = SqlStatementSplitter.init(m.sql);
        while (splitter.next()) |stmt| {
            const s = try stripped(alloc, stmt);
            defer alloc.free(s);
            if (std.mem.indexOf(u8, s, T_LEDGER) == null) continue;
            if (std.mem.indexOf(u8, s, TO_API) != null) seen = true;
        }
    }
    try std.testing.expect(seen);
}

const Footprint = struct { seen: [METERING_FLEET_TABLES.len]bool = .{false} ** METERING_FLEET_TABLES.len };

fn checkMeteringFootprint(f: *Footprint, version: i32, s: []const u8) !void {
    if (std.mem.indexOf(u8, s, TO_METERING) == null) return;
    // Object grants only: a membership carries no table and a schema USAGE grant
    // names none either. Both are asserted separately.
    if (isMembershipGrant(s)) return;
    if (std.mem.indexOf(u8, s, " ON SCHEMA ") != null) return;

    var matched = false;
    for (METERING_FLEET_TABLES, 0..) |table, i| {
        if (std.mem.indexOf(u8, s, table) == null) continue;
        f.seen[i] = true;
        matched = true;
    }
    std.testing.expect(matched) catch |err| {
        std.debug.print(
            "\nFAIL: slot v{d} widens metering_runtime past the fenced statement's " ++
                "footprint, so its reach stops being enumerable: {s}\n",
            .{ version, s },
        );
        return err;
    };
    try std.testing.expect(std.mem.indexOf(u8, s, T_WALLET) == null);
    try std.testing.expect(std.mem.indexOf(u8, s, T_LEDGER) == null);
}

test "metering_runtime reaches exactly the fenced statement's fleet tables" {
    var footprint = Footprint{};
    try forEachGrant(&footprint, checkMeteringFootprint);
    for (METERING_FLEET_TABLES, footprint.seen) |table, granted| {
        std.testing.expect(granted) catch |err| {
            std.debug.print("\nFAIL: metering_runtime lacks {s}\n", .{table});
            return err;
        };
    }
}

fn checkMeteringInheritsBilling(c: *Counter, _: i32, s: []const u8) !void {
    if (!isMembershipGrant(s)) return;
    if (std.mem.indexOf(u8, s, R_BILLING) == null) return;
    if (std.mem.indexOf(u8, s, TO_METERING) == null) return;
    // The mirror image of the api_runtime rule: the fenced statement cannot stop
    // to elevate a second time, so THIS membership must inherit.
    try std.testing.expect(std.mem.indexOf(u8, s, "INHERIT FALSE") == null);
    c.n += 1;
}

test "metering_runtime inherits the wallet rather than restating its grants" {
    var counter = Counter{};
    try forEachGrant(&counter, checkMeteringInheritsBilling);
    try std.testing.expectEqual(@as(usize, 1), counter.n);

    // The membership only carries the wallet if the role itself inherits, and
    // that attribute is load-bearing enough to be stated rather than defaulted:
    // NOINHERIT would strip the wallet mid-statement and fail on the money path
    // at runtime, not at bootstrap. Asserted on the CREATE, not just the GRANT.
    var declared = false;
    for (schema.migrations) |m| {
        if (std.mem.indexOf(u8, m.sql, "CREATE ROLE " ++ R_METERING) == null) continue;
        try std.testing.expect(
            std.mem.indexOf(u8, m.sql, "CREATE ROLE " ++ R_METERING ++ " NOLOGIN INHERIT") != null,
        );
        declared = true;
    }
    try std.testing.expect(declared);
}
