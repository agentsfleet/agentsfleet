//! The command-line credential endpoints, driven over the real router.
//!
//! The unit suite beside the handler proves its pure decisions — which
//! principal modes are admitted, and what a machine name may look like. Every
//! one of those tests stops at a guard that returns before any datastore work,
//! deliberately, because a stubbed connection cannot check a WHERE clause.
//!
//! This suite is where the WHERE clauses are checked. The ownership predicates
//! live inside the statements rather than in a handler branch, so the only way
//! to prove them is to put two people's rows in one table under one tenant and
//! confirm that each reaches exactly their own. That is what the peer seeded
//! here is for: same tenant, no token, and a credential the owner must not be
//! able to see or retire.
//!
//! Requires TEST_DATABASE_URL — skipped gracefully otherwise.

const std = @import("std");

const fixtures = @import("cli_credentials_test_fixtures.zig");
const api_key = @import("../../../auth/api_key.zig");
const cli_credential = @import("../../../auth/cli_credential.zig");
const ec = @import("../../../errors/error_registry.zig");

const ALLOC = fixtures.ALLOC;
const PATH = fixtures.PATH;

fn revokePath(credential_id: []const u8) ![]const u8 {
    return std.fmt.allocPrint(ALLOC, "{s}/{s}", .{ PATH, credential_id });
}

test "integration: test_credential_resolves_to_its_user — a credential reaches its own person's rows and no one else's" {
    const h = fixtures.seededHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const minted = try fixtures.mint(h, fixtures.TOKEN_OWNER, fixtures.MACHINE_NAME);
    defer minted.deinit();

    // The peer holds a credential under the SAME tenant. A principal resolved
    // tenant-wide would reach it; one resolved to a person cannot.
    const peer = blk: {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        break :blk try fixtures.mintDirect(conn, fixtures.PEER_USER_ID, fixtures.MACHINE_NAME);
    };
    defer peer.deinit(ALLOC);

    {
        // The credential authenticates, and lists exactly its own person's row.
        const r = try (try h.get(PATH).bearer(minted.secret)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
        try std.testing.expect(r.bodyContains(minted.id));
        try std.testing.expect(r.bodyContains(fixtures.MACHINE_NAME));
        // The whole point: the peer's row is in the same table under the same
        // tenant, and must not appear.
        try std.testing.expect(!r.bodyContains(peer.id));
    }

    {
        // Nor may it retire the peer's credential. The refusal is not-found
        // rather than forbidden — telling the two apart would confirm that
        // somebody else's credential exists to whoever guessed its identifier.
        const path = try revokePath(peer.id);
        defer ALLOC.free(path);
        const r = try (try h.delete(path).bearer(minted.secret)).send();
        defer r.deinit();
        try r.expectStatus(.not_found);
        try r.expectErrorCode(ec.ERR_CLI_CREDENTIAL_NOT_FOUND);
    }

    {
        // And the peer's credential is still live afterwards — the refusal was
        // a refusal, not a silent revoke that reported not-found.
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try std.testing.expectEqual(
            @as(i64, 1),
            try fixtures.liveCountForMachine(conn, fixtures.PEER_USER_ID, fixtures.MACHINE_NAME),
        );
    }

    fixtures.cleanup(h);
}

test "integration: test_row_holds_no_recoverable_credential — the stored row cannot reconstruct what was issued" {
    const h = fixtures.seededHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const minted = try fixtures.mint(h, fixtures.TOKEN_OWNER, fixtures.MACHINE_NAME);
    defer minted.deinit();

    const row = try fixtures.wholeRow(h, minted.id);
    defer ALLOC.free(row);

    // Every column, in one string: the raw value appears in none of them.
    try std.testing.expect(std.mem.indexOf(u8, row, minted.secret) == null);
    // What IS stored is a digest OF it — present, and not the value itself.
    const digest = api_key.sha256Hex(minted.secret);
    try std.testing.expect(std.mem.indexOf(u8, row, digest[0..]) != null);
    // The display fragment is stored, and is a strict prefix — recognisable in
    // a list, useless as a credential.
    const shown = cli_credential.displayPrefix(minted.secret);
    try std.testing.expect(std.mem.indexOf(u8, row, shown) != null);
    try std.testing.expect(shown.len < minted.secret.len);

    {
        // The digest is not a bearer token. Presenting it hashes it a second
        // time, which matches nothing — so reading the row grants nothing.
        const r = try (try h.get(PATH).bearer(digest[0..])).send();
        defer r.deinit();
        try r.expectStatus(.unauthorized);
    }
    {
        // Neither is the display fragment, which is the part a list DOES show.
        const r = try (try h.get(PATH).bearer(shown)).send();
        defer r.deinit();
        try r.expectStatus(.unauthorized);
        try r.expectErrorCode(ec.ERR_UNAUTHORIZED);
    }

    fixtures.cleanup(h);
}

test "integration: test_revoked_credential_is_refused — a retired credential answers its own code, not a generic refusal" {
    const h = fixtures.seededHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const minted = try fixtures.mint(h, fixtures.TOKEN_OWNER, fixtures.MACHINE_NAME);
    defer minted.deinit();

    {
        const r = try (try h.get(PATH).bearer(minted.secret)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
    }

    const path = try revokePath(minted.id);
    defer ALLOC.free(path);
    {
        const r = try (try h.delete(path).bearer(fixtures.TOKEN_OWNER)).send();
        defer r.deinit();
        try r.expectStatus(.no_content);
        try std.testing.expectEqual(@as(usize, 0), r.body.len);
    }
    {
        // The distinction that matters to an operator: this credential was
        // retired, not mistyped. A generic 401 would send them hunting for a
        // typo instead of running login again.
        const r = try (try h.get(PATH).bearer(minted.secret)).send();
        defer r.deinit();
        try r.expectErrorCode(ec.ERR_CLI_CREDENTIAL_REVOKED);
    }
    {
        // Revoking it twice does not re-revoke: the statement only touches live
        // rows, so the original revocation timestamp survives.
        const r = try (try h.delete(path).bearer(fixtures.TOKEN_OWNER)).send();
        defer r.deinit();
        try r.expectStatus(.not_found);
        try r.expectErrorCode(ec.ERR_CLI_CREDENTIAL_NOT_FOUND);
    }

    fixtures.cleanup(h);
}

test "integration: an unsupported method on either credential route answers 405, not 404 or 500" {
    // The collection accepts POST and GET; the item form accepts only DELETE.
    // Both refusals live in the invoke dispatch, past routing and past auth, so
    // this also proves the router reached the right handler: a matcher that
    // claimed the wrong shape would answer 404 here instead.
    const h = fixtures.seededHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    {
        const r = try (try (try h.put(PATH).bearer(fixtures.TOKEN_OWNER)).json("{}")).send();
        defer r.deinit();
        try r.expectStatus(.method_not_allowed);
    }
    {
        // A syntactically valid identifier, so the refusal is about the method
        // rather than the shape of the path segment.
        const path = try revokePath(fixtures.OWNER_USER_ID);
        defer ALLOC.free(path);
        const r = try (try (try h.post(path).bearer(fixtures.TOKEN_OWNER)).json("{}")).send();
        defer r.deinit();
        try r.expectStatus(.method_not_allowed);
    }

    fixtures.cleanup(h);
}

test "integration: test_tenant_key_refused_on_user_scoped_route — an organisation cannot act as a person" {
    const h = fixtures.seededHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // The tenant key authenticates — it is a real, active key — and is then
    // refused on principal MODE. That ordering is the test: a key that failed
    // to authenticate would answer 401 and prove nothing about the guard.
    const body = "{\"machine_name\":\"" ++ fixtures.MACHINE_NAME ++ "\"}";
    {
        const r = try (try (try h.post(PATH).bearer(fixtures.TENANT_KEY)).json(body)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
        try r.expectErrorCode(ec.ERR_FORBIDDEN);
    }
    {
        const r = try (try h.get(PATH).bearer(fixtures.TENANT_KEY)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
        try r.expectErrorCode(ec.ERR_FORBIDDEN);
    }
    {
        const path = try revokePath(fixtures.OWNER_USER_ID);
        defer ALLOC.free(path);
        const r = try (try h.delete(path).bearer(fixtures.TENANT_KEY)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
        try r.expectErrorCode(ec.ERR_FORBIDDEN);
    }

    {
        // The refused mint wrote nothing. A 403 that still inserted would make
        // the audit trail name a person for an organisation's action.
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try std.testing.expectEqual(
            @as(i64, 0),
            try fixtures.liveCountForMachine(conn, fixtures.OWNER_USER_ID, fixtures.MACHINE_NAME),
        );
    }

    fixtures.cleanup(h);
}

test "integration: test_credential_cannot_mint_another_credential — a credential is not a key to more credentials" {
    const h = fixtures.seededHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // A real, live credential for this person's own machine. It authenticates,
    // so every assertion below is about what it is permitted to DO — a value
    // that failed to authenticate would answer 401 and prove nothing.
    const conn = try h.acquireConn();
    const owned = try fixtures.mintDirect(conn, fixtures.OWNER_USER_ID, fixtures.MACHINE_NAME);
    defer owned.deinit(ALLOC);
    h.releaseConn(conn);

    // Minting under a machine name of the caller's choosing is the step that
    // would turn one stolen credential into an unbounded, self-renewing
    // supply: each mint yields the next, revoking any single row leaves its
    // siblings live, and the person holding the account cannot tell how many
    // exist. The browser sign-in is the cost an attacker cannot replay, so
    // minting keeps it and a credential is refused here.
    const body = "{\"machine_name\":\"" ++ fixtures.OTHER_MACHINE_NAME ++ "\"}";
    {
        const r = try (try (try h.post(PATH).bearer(owned.secret)).json(body)).send();
        defer r.deinit();
        try r.expectStatus(.forbidden);
        try r.expectErrorCode(ec.ERR_FORBIDDEN);
    }

    {
        // The refusal wrote nothing: the second machine holds no live row, so
        // the chain does not start even once.
        const c = try h.acquireConn();
        defer h.releaseConn(c);
        try std.testing.expectEqual(
            @as(i64, 0),
            try fixtures.liveCountForMachine(c, fixtures.OWNER_USER_ID, fixtures.OTHER_MACHINE_NAME),
        );
    }

    // The same credential still manages its own existence. Listing stays open
    // precisely so a terminal can see and end its own access without opening a
    // browser; narrowing the mint must not have narrowed that too.
    {
        const r = try (try h.get(PATH).bearer(owned.secret)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
    }

    fixtures.cleanup(h);
}
