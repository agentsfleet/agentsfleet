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
const common = @import("common");

const fixtures = @import("cli_credentials_test_fixtures.zig");
const ec = @import("../../../errors/error_registry.zig");
const api_key = @import("../../../auth/api_key.zig");
const cli_credential_lookup = @import("../../../cmd/cli_credential_lookup.zig");

const ALLOC = fixtures.ALLOC;

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
        const r = try (try h.get(fixtures.PROBE_PATH).bearer(first.secret)).send();
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
        const r = try (try h.get(fixtures.PROBE_PATH).bearer(second.secret)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
    }

    // And the superseded credential is refused with the code that tells the
    // operator to log in again, not a generic refusal they cannot act on.
    {
        const r = try (try h.get(fixtures.PROBE_PATH).bearer(first.secret)).send();
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
        const r = try (try h.get(fixtures.PROBE_PATH).bearer(desktop.secret)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
    }

    fixtures.cleanup(h);
}

// ── The principal's tenant is the user row's, never the mint snapshot ────────
// uuidv7-shaped (version nibble 7), disjoint from the fixtures' id block.
const DIVERGENT_TENANT_ID = "0195c9aa-7d0a-7f13-8abc-2b3e1e0a7d01";

test "integration: test_principal_tenant_is_the_users_row — a divergent mint snapshot cannot steer authorization" {
    const h = fixtures.seededHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const minted = try fixtures.mint(h, fixtures.TOKEN_OWNER, fixtures.MACHINE_NAME);
    defer minted.deinit();

    // Point the credential ROW's tenant snapshot at a different, freshly
    // seeded tenant. The row column is provenance; if the auth lookup ever
    // reads it as authority again, this test screams. The undo is DEFERRED so
    // a failing assertion cannot strand the shared fixtures with a credential
    // row pointed at the divergent tenant.
    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        const now_ms = common.clock.nowMillis();
        _ = try conn.exec(
            \\INSERT INTO core.tenants (id, name, created_at, updated_at)
            \\VALUES ($1::uuid, 'CLI Credential Divergent Tenant', $2::bigint, $2::bigint)
            \\ON CONFLICT (id) DO NOTHING
        , .{ DIVERGENT_TENANT_ID, now_ms });
        _ = try conn.exec(
            "UPDATE core.cli_credentials SET tenant_id = $1::uuid WHERE id = $2::uuid",
            .{ DIVERGENT_TENANT_ID, minted.id },
        );
    }
    defer {
        const conn = h.acquireConn() catch null;
        if (conn) |c| {
            defer h.releaseConn(c);
            _ = c.exec(
                "UPDATE core.cli_credentials SET tenant_id = $1::uuid WHERE id = $2::uuid",
                .{ fixtures.TENANT_ID, minted.id },
            ) catch {};
            _ = c.exec("DELETE FROM core.tenants WHERE id = $1::uuid", .{DIVERGENT_TENANT_ID}) catch {};
        }
    }

    // The changed SELECT under proof: the lookup returns the JOINED user
    // row's tenant, not the credential row's snapshot.
    var lookup_ctx = cli_credential_lookup.Ctx{ .pool = h.pool };
    const hash = api_key.sha256Hex(minted.secret);
    const row = (try cli_credential_lookup.lookup(@ptrCast(&lookup_ctx), ALLOC, hash[0..])) orelse
        return error.TestUnexpectedResult;
    defer {
        ALLOC.free(row.credential_id);
        ALLOC.free(row.user_id);
        ALLOC.free(row.tenant_id);
        ALLOC.free(row.deployment);
        ALLOC.free(row.oidc_subject);
    }
    try std.testing.expectEqualStrings(fixtures.TENANT_ID, row.tenant_id);

    // And the full request path still authorizes under the user's tenant.
    {
        const r = try (try h.get(fixtures.PROBE_PATH).bearer(minted.secret)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
    }

    fixtures.cleanup(h);
}
