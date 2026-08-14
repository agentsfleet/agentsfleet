//! Who the command-line credential routes admit, driven over the real router.
//!
//! Its sibling `cli_credentials_integration_test.zig` proves what an admitted
//! credential reaches — the ownership predicates inside the statements. This
//! suite asks the earlier question: which callers get that far at all. The two
//! split because they fail for different reasons and are read at different
//! times; a widened guard here and a widened WHERE clause there are separate
//! regressions with separate fixes.
//!
//! Every refusal below is on principal MODE, and every principal that meets one
//! authenticates first. That ordering is the point throughout: a credential
//! that failed to authenticate would answer 401 and prove nothing about the
//! guard the test is named for.
//!
//! Requires TEST_DATABASE_URL — skipped gracefully otherwise.

const std = @import("std");

const fixtures = @import("cli_credentials_test_fixtures.zig");
const ec = @import("../../../errors/error_registry.zig");
const handler = @import("cli_credentials.zig");

const ALLOC = fixtures.ALLOC;
const PATH = fixtures.PATH;
const revokePath = fixtures.revokePath;

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
        // Named, not merely coded: the person guard beside this one answers the
        // same code, so the code alone would still hold after the freshness
        // guard was deleted. The unit tier pins the same string without a
        // datastore, which is what makes the guard mutation-testable at all.
        try std.testing.expect(r.bodyContains(handler.S_SESSION_REQUIRED));
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
