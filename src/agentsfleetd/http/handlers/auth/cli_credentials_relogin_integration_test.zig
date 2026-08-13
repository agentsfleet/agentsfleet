//! What a SECOND login leaves behind — for this machine, and for every other.
//!
//! The sibling index suite proves what the datastore refuses when something
//! skips the revoke. This one asks the ordinary question an operator asks: they
//! log in again, and the terminal must end up holding exactly one credential
//! that works — while the laptop in the other room keeps working too, because
//! its credential was never in the revoke's predicate.
//!
//! Both claims are about what survives ACROSS two logins, so neither is
//! observable from a single mint: the row counts, the surviving row's bytes,
//! and which secret still authenticates are three different facts, and a test
//! that checked only the first would pass with the wrong credential alive.
//!
//! Requires TEST_DATABASE_URL — skipped gracefully otherwise.

const std = @import("std");

const fixtures = @import("cli_credentials_test_fixtures.zig");
const ec = @import("../../../errors/error_registry.zig");

const ALLOC = fixtures.ALLOC;
const PATH = fixtures.PATH;

test "integration: test_relogin_leaves_one_live_credential — logging in twice holds one credential, not two" {
    const h = fixtures.seededHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const first = try fixtures.mint(h, fixtures.TOKEN_OWNER, fixtures.MACHINE_NAME);
    defer first.deinit();

    // Established BEFORE the second login, so the refusal at the end is the
    // re-login's doing rather than a credential that never worked at all.
    {
        const r = try (try h.get(PATH).bearer(first.secret)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
    }

    const second = try fixtures.mint(h, fixtures.TOKEN_OWNER, fixtures.MACHINE_NAME);
    defer second.deinit();

    // Revoke-then-mint, not reuse: a credential's secret is returned once at
    // creation and cannot be recovered afterwards, so a second login that
    // "kept" the existing row could only hand back something unusable.
    try std.testing.expect(!std.mem.eql(u8, first.id, second.id));
    try std.testing.expect(!std.mem.eql(u8, first.secret, second.secret));

    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try std.testing.expectEqual(
            @as(i64, 1),
            try fixtures.liveCountForMachine(conn, fixtures.OWNER_USER_ID, fixtures.MACHINE_NAME),
        );
    }

    // The count alone would pass if the SURVIVOR were the wrong row. The
    // terminal walked away holding `second`, so that is the one that must work.
    {
        const r = try (try h.get(PATH).bearer(second.secret)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
    }

    // And the superseded credential is refused with the code that tells the
    // operator to log in again, not a generic refusal they cannot act on.
    {
        const r = try (try h.get(PATH).bearer(first.secret)).send();
        defer r.deinit();
        try r.expectErrorCode(ec.ERR_CLI_CREDENTIAL_REVOKED);
    }

    fixtures.cleanup(h);
}

test "integration: test_other_machines_credential_survives_login — the desktop is not collateral damage" {
    const h = fixtures.seededHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const desktop = try fixtures.mint(h, fixtures.TOKEN_OWNER, fixtures.OTHER_MACHINE_NAME);
    defer desktop.deinit();

    const before = try fixtures.wholeRow(h, desktop.id);
    defer ALLOC.free(before);

    // A login from a DIFFERENT machine, by the same person. The revoke it runs
    // first is scoped to one (user, machine) pair, so this row is not in its
    // predicate — which is the whole reason a second laptop keeps working.
    const laptop = try fixtures.mint(h, fixtures.TOKEN_OWNER, fixtures.MACHINE_NAME);
    defer laptop.deinit();

    const after = try fixtures.wholeRow(h, desktop.id);
    defer ALLOC.free(after);

    // Byte-identical, which covers `revoked_at` without singling it out: a
    // revoke that over-reached by one column would surface here.
    try std.testing.expectEqualStrings(before, after);

    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try std.testing.expectEqual(
            @as(i64, 1),
            try fixtures.liveCountForMachine(conn, fixtures.OWNER_USER_ID, fixtures.OTHER_MACHINE_NAME),
        );
        try std.testing.expectEqual(
            @as(i64, 1),
            try fixtures.liveCountForMachine(conn, fixtures.OWNER_USER_ID, fixtures.MACHINE_NAME),
        );
    }

    // The claim in the operator's terms. A row that survives but no longer
    // authenticates is not survival, so the assertion is a real request.
    {
        const r = try (try h.get(PATH).bearer(desktop.secret)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
    }

    fixtures.cleanup(h);
}
