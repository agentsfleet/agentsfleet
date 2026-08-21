//! Allocation-failure proofs for the signup bootstrap and the teardown scan.
//!
//! Three reads on one identity: the bootstrap that creates a personal account,
//! the lookup that replays an existing one, and the fleet scan account deletion
//! runs before it purges. Each builds owned strings behind `errdefer` rungs
//! that only run when a LATER allocation fails.
//!
//! ## Why the bootstrap wrapper deletes first
//!
//! `checkAllAllocationFailures` runs the function once on a working allocator
//! to count its allocation sites, and THAT run commits a real account. Every
//! failing run afterwards would find the account already there, return through
//! `replayExisting`, and never reach the workspace-id, workspace-name or
//! candidate-name rungs — a green proof over three rungs it never touched.
//! Deleting the identity at the top of each run is what keeps every run a
//! genuine create. The deletes go through the connection, never the failing
//! allocator, so they cannot shift the allocation indices being walked.
//!
//! The other two reads need no such reset: both are pure reads, so a fixture
//! seeded once stays correct for every run.

const std = @import("std");
const pg = @import("pg");

const signup_bootstrap = @import("signup_bootstrap.zig");
const store = @import("signup_bootstrap_store.zig");
const account_teardown = @import("account_teardown.zig");
const base = @import("../db/test_fixtures.zig");

const ALLOC = std.testing.allocator;

const OIDC_CREATE = "oidc-signup-alloc-create-01";
const OIDC_READ = "oidc-signup-alloc-read-02";
const EMAIL_CREATE = "signup-alloc-create@acme.test";
const EMAIL_READ = "signup-alloc-read@acme.test";
const DISPLAY_NAME = "Signup Alloc Proof";

/// Two fleets, so the teardown scan's ladder fails with ids already owned —
/// a one-fleet fixture can only fail on an empty list.
const FLEET_ONE = "0195b4ba-8d3a-7f13-8abc-0000000ad001";
const FLEET_TWO = "0195b4ba-8d3a-7f13-8abc-0000000ad002";

/// Drop one identity and everything hanging off it, in FK-safe order. Uses the
/// connection only — nothing here may allocate from the failing allocator.
/// Every purge statement binds the same single parameter, so they differ only
/// in their SQL. Listing them as data and walking the list keeps the FK-safe
/// ORDER visible in one place — the thing that actually matters here — instead
/// of spreading it across five near-identical exec-and-swallow blocks.
const PURGE_BY_OIDC = [_][]const u8{
    \\DELETE FROM core.fleets WHERE workspace_id IN (
    \\  SELECT id FROM core.workspaces WHERE tenant_id IN (
    \\    SELECT tenant_id FROM core.users WHERE oidc_subject = $1))
    ,
    \\DELETE FROM core.workspaces WHERE tenant_id IN (
    \\  SELECT tenant_id FROM core.users WHERE oidc_subject = $1)
    ,
    \\DELETE FROM core.memberships WHERE user_id IN (
    \\  SELECT id FROM core.users WHERE oidc_subject = $1)
    ,
    \\DELETE FROM billing.tenant_wallet WHERE tenant_id IN (
    \\  SELECT tenant_id FROM core.users WHERE oidc_subject = $1)
    ,
    \\WITH doomed AS (
    \\  DELETE FROM core.users WHERE oidc_subject = $1 RETURNING tenant_id
    \\)
    \\DELETE FROM core.tenants WHERE id IN (SELECT tenant_id FROM doomed)
    ,
};

/// Drop one identity and everything hanging off it, in FK-safe order. Uses the
/// connection only — nothing here may allocate from the failing allocator.
fn dropIdentity(conn: *pg.Conn, oidc_subject: []const u8) void {
    for (PURGE_BY_OIDC) |stmt| {
        _ = conn.exec(stmt, .{oidc_subject}) catch |err|
            std.log.warn("ignored: {s}", .{@errorName(err)});
    }
}

/// Bootstrap the read fixture once and hang two fleets off its workspace.
fn seedReadFixture(conn: *pg.Conn) !void {
    dropIdentity(conn, OIDC_READ);
    var b = try signup_bootstrap.bootstrapPersonalAccount(conn, ALLOC, .{
        .oidc_subject = OIDC_READ,
        .email = EMAIL_READ,
        .display_name = DISPLAY_NAME,
    });
    defer b.deinit(ALLOC);
    try base.seedFleet(conn, FLEET_ONE, b.workspace_id, "signup-alloc-one", "{}", "# one");
    try base.seedFleet(conn, FLEET_TWO, b.workspace_id, "signup-alloc-two", "{}", "# two");
}

// ── Wrappers ──────────────────────────────────────────────────────────────

/// Fixed-length name, injected through the seam `bootstrapTransaction` already
/// exposes for tests. Production's `defaultHerokuNameGen` returns a RANDOM name
/// of varying length, so the byte count differs between runs and
/// `checkAllAllocationFailures` rejects the whole proof as
/// `NondeterministicMemoryUsage` before it can fail a single site. A constant
/// name also lands on the first attempt, which keeps
/// `pickUniqueWorkspaceName`'s retry loop from varying the count on top.
fn fixedWorkspaceName(alloc: std.mem.Allocator) anyerror![]u8 {
    return alloc.dupe(u8, "signup-alloc-fixed-name");
}

/// Drives `bootstrapTransaction` rather than `bootstrapPersonalAccount`: the
/// public wrapper hard-codes the random generator, and the replay fast-path it
/// adds is dead here anyway because every run starts from a dropped identity.
fn bootstrapUnderAllocator(alloc: std.mem.Allocator, conn: *pg.Conn) !void {
    dropIdentity(conn, OIDC_CREATE);
    var b = try signup_bootstrap.bootstrapTransaction(conn, alloc, .{
        .oidc_subject = OIDC_CREATE,
        .email = EMAIL_CREATE,
        .display_name = DISPLAY_NAME,
    }, fixedWorkspaceName);
    b.deinit(alloc);
}

fn findExistingUnderAllocator(alloc: std.mem.Allocator, conn: *pg.Conn) !void {
    const existing = (try store.findExistingByOidcSubject(conn, alloc, OIDC_READ)) orelse
        return error.FixtureAccountMissing;
    alloc.free(existing.user_id);
    alloc.free(existing.tenant_id);
    alloc.free(existing.workspace_id);
    alloc.free(existing.workspace_name);
}

fn fleetIdsUnderAllocator(alloc: std.mem.Allocator, conn: *pg.Conn) !void {
    const ids = (try account_teardown.fleetIdsByOidcSubject(conn, alloc, OIDC_READ)) orelse
        return error.FixtureFleetsMissing;
    for (ids) |id| alloc.free(id);
    alloc.free(ids);
}

// ── Proofs ────────────────────────────────────────────────────────────────

test "integration: every allocation site in the signup bootstrap unwinds without leaking" {
    const db = (try base.TestDb.open(ALLOC)) orelse return error.SkipZigTest;
    defer db.close();
    defer dropIdentity(db.conn, OIDC_CREATE);

    try std.testing.checkAllAllocationFailures(
        ALLOC,
        bootstrapUnderAllocator,
        .{db.conn},
    );
}

test "integration: every allocation site in the identity replay read unwinds without leaking" {
    const db = (try base.TestDb.open(ALLOC)) orelse return error.SkipZigTest;
    defer db.close();

    try seedReadFixture(db.conn);
    defer dropIdentity(db.conn, OIDC_READ);

    try std.testing.checkAllAllocationFailures(
        ALLOC,
        findExistingUnderAllocator,
        .{db.conn},
    );
}

test "integration: every allocation site in the teardown fleet scan unwinds without leaking" {
    const db = (try base.TestDb.open(ALLOC)) orelse return error.SkipZigTest;
    defer db.close();

    try seedReadFixture(db.conn);
    defer dropIdentity(db.conn, OIDC_READ);

    try std.testing.checkAllAllocationFailures(
        ALLOC,
        fleetIdsUnderAllocator,
        .{db.conn},
    );
}
