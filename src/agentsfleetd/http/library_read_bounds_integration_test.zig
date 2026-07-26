//! Integration tier for §3 Dimension 3.2 — `test_library_read_resource_bounds`.
//!
//! §3 states a numeric table: per library read path, how many database
//! statements, decryptions, results, encoded body bytes, and pooled connections
//! it may cost. A table nobody measures is a comment, and the specific way it
//! rots is silent — someone adds a helper that resolves a workspace, and a page
//! that cost five statements costs six without a single test noticing.
//!
//! This file measures the table. **Only the tenant registry row is asserted
//! here**, because it is the only library read path that exists: the global
//! models page, the Fleet summary, and the Fleet detail are §§2–3 handlers not
//! yet built. Their rows land beside this one when they do — asserting them now
//! would mean asserting a budget for a route that returns 404, which passes for
//! the wrong reason and would keep passing after the handler arrives.
//!
//! The `UZ-LIBRARY-005` ceiling refusal and the `model_id` write bound live in
//! `library_body_ceiling_integration_test.zig` — split off at the 350-line cap
//! (RULE FLL), along the seam between what a read COSTS and what an over-ceiling
//! response DOES.
//!
//! ## Why this drives the HTTP route rather than `view.buildList`
//!
//! Two of the five columns do not exist below the handler. The connection is
//! acquired by the handler, and the encoded body is produced by it; a test that
//! calls `buildList` directly measures three columns and silently reports zero
//! for the other two — the shape of vacuous pass this file exists to avoid.
//! Driving the real route also puts the measurement window exactly where §3
//! puts it: `beginRead()` opens at handler entry, so the bearer chain's own
//! statements are outside the budget, which is what "after middleware auth"
//! means.
//!
//! ## Why the numbers are exact, not upper bounds
//!
//! `statements == 6` fails when the read grows a seventh. `statements <= 6` also
//! fails then, but an exact count additionally fails when a statement is
//! REMOVED — which is how a page stops resolving the platform default and
//! nobody notices until the Default row renders empty in production.
//!
//! `LIVE_DB=1` + `TEST_DATABASE_URL` (set by `make test-integration`);
//! self-skips otherwise.

const std = @import("std");

const base = @import("secrets_json_integration_test.zig");
const fixtures = @import("library_bounds_test_fixtures.zig");
const counters = @import("../observability/library_read_counters.zig");
const crypto_store = @import("../secrets/crypto_store.zig");
const fixtures_provider = @import("../db/test_fixtures_provider.zig");

/// The tenant registry page's six statements, enumerated so a failure of the
/// exact-count assertion can be read against the list rather than rediscovered:
///
///   1. `tenant_provider.activeSelfManagedRef`  — which entry is active
///   2. `entries_state.listPage`                — the page itself (over-fetch by one)
///   3. `secret_probe.resolvePrimaryWorkspace`  — once for the page, not per row
///   4. `vault.loadMetadata`                    — one batch for every row's metadata
///   5. `tenant_provider.platformDefaultView`   — the Default row's identity
///   6. `model_rate_cache.loadRatesForPairs`    — one batch for every row's rate,
///                                                plus the default's, in the
///                                                same statement
///
/// Independent of page size by construction: 3, 4 and 6 are the three a naive
/// per-row projection would multiply, and all three are hoisted. That
/// independence is the claim §3's budget makes; the absolute number is only the
/// current measurement of it, and has been corrected upward twice as the read
/// gained fields it renders.
const EXPECTED_STATEMENTS: usize = 6;

/// Every claim §3's tenant-registry row makes, checked against one response.
///
/// `body_len` is the bytes the client actually received. Comparing the tally to
/// it is what stops `noteEncodedBytes` from being self-certifying: the handler
/// measures the body it is ABOUT to write, so a measurement taken with
/// different serialization options than the write would sail through a bare
/// "under the ceiling" assertion while describing a different body.
fn expectRegistryBudget(measured: counters.Snapshot, body_len: usize, want_results: usize) !void {
    try std.testing.expectEqual(EXPECTED_STATEMENTS, measured.statements);
    try std.testing.expectEqual(@as(usize, 0), crypto_store.decryptCountForTest());
    try std.testing.expectEqual(want_results, measured.results);
    try std.testing.expectEqual(counters.MAX_CONNECTIONS_PER_READ, measured.connections);
    try std.testing.expectEqual(body_len, measured.encoded_bytes);

    // The §3 ceilings themselves. Asserted against the named constants rather
    // than literals so relaxing one has to happen in the module that owns the
    // table, where the pin test will see it.
    try std.testing.expect(measured.statements <= counters.TENANT_REGISTRY_MAX_STATEMENTS);
    try std.testing.expect(measured.results <= counters.TENANT_REGISTRY_MAX_RESULTS);
    try std.testing.expect(measured.encoded_bytes <= counters.TENANT_REGISTRY_MAX_BODY_BYTES);

    // Non-vacuity: every equality above would also hold against a counter that
    // never fired, if the expected value happened to be zero. It is not — but
    // the assertion that says so belongs here rather than in a reviewer's head.
    try std.testing.expect(measured.encoded_bytes > 0);
}

test "integration: test_library_read_resource_bounds — the tenant registry page stays inside §3's table" {
    base.setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = base.seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    {
        // A sibling suite's leftover entries would still satisfy every ceiling,
        // but they would break the exact result count below — and an exact
        // count is the only assertion that catches a page returning MORE than
        // its limit.
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        base.cleanupRows(conn);
        // The rate batch's size — and therefore whether it runs at all — depends
        // on the platform default, so this suite owns that state rather than
        // inheriting whatever a sibling left active. Seeded here, not merely
        // cleared: statement 6 must be exercised with a pair to resolve.
        try fixtures_provider.seedPlatformProvider(alloc, conn, base.TEST_WS_ID);
    }
    defer {
        if (h.acquireConn()) |conn| {
            fixtures_provider.teardownPlatformProvider(conn, base.TEST_WS_ID);
            h.releaseConn(conn);
        } else |err| {
            std.log.warn("platform default teardown skipped: {s}", .{@errorName(err)});
        }
    }
    try fixtures.seedEntries(alloc, h);

    // ── a truncated page: the tally is bounded by `limit`, not by row count ──
    {
        const path = std.fmt.comptimePrint("{s}?limit={d}", .{ fixtures.MODELS_PATH, fixtures.SMALL_PAGE });
        crypto_store.resetDecryptCountForTest();
        const r = try (try h.get(path).bearer(base.TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.ok);

        // The window closed when the handler returned, so this snapshot is that
        // request's and nothing else's.
        try expectRegistryBudget(counters.snapshot(), r.body.len, fixtures.SMALL_PAGE);

        // The page really was short — otherwise `results == SMALL_PAGE` could
        // mean the seed failed rather than that the limit held.
        try std.testing.expect(!r.bodyContains("\"next_cursor\":null"));
    }

    // ── the full page: same six statements, more rows, still zero decrypts ──
    {
        crypto_store.resetDecryptCountForTest();
        const r = try (try h.get(fixtures.MODELS_PATH).bearer(base.TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.ok);

        // The budget does not scale with page size. That is the whole claim of
        // §1's projection rewrite, and it is the one a per-row read would break
        // here while still passing every assertion in the block above.
        try expectRegistryBudget(counters.snapshot(), r.body.len, fixtures.SEEDED_ENTRIES);
        try std.testing.expect(r.bodyContains("\"next_cursor\":null"));

        // The rates the sixth statement went and got. A resident-only reader
        // returns null for every one of these until some unrelated billing
        // charge happens to load the exact pair — which is precisely the
        // regression this row's correction fixed, and a statement-count
        // assertion alone would not have noticed it.
        try std.testing.expect(r.bodyContains("\"context_cap_tokens\":"));
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    base.cleanupRows(conn);
}

test "integration: test_library_read_resource_bounds — an empty registry costs less, never more" {
    base.setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = base.seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        base.cleanupRows(conn);
        // Explicit, not inherited. Both statements this test proves absent are
        // skipped for the same reason — nothing to ask about — and a platform
        // default left active by a sibling suite would supply the rate batch a
        // pair, making the count depend on execution order rather than on the
        // guards under test.
        fixtures_provider.teardownPlatformProvider(conn, base.TEST_WS_ID);
    }

    crypto_store.resetDecryptCountForTest();
    const r = try (try h.get(fixtures.MODELS_PATH).bearer(base.TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);

    const measured = counters.snapshot();

    // Four, not six: `vault.loadMetadata` and `loadRatesForPairs` BOTH return
    // before querying when there is nothing to describe or price. Pinned rather
    // than waved through as "under the ceiling" because the degenerate page is
    // where a guard clause gets deleted during a refactor and nobody notices
    // the extra statement — it is, after all, still under budget.
    try std.testing.expectEqual(EXPECTED_STATEMENTS - 2, measured.statements);
    try std.testing.expectEqual(@as(usize, 0), measured.results);
    try std.testing.expectEqual(@as(usize, 0), crypto_store.decryptCountForTest());
    try std.testing.expectEqual(counters.MAX_CONNECTIONS_PER_READ, measured.connections);

    // An empty page is still a body — envelope, `total`, `next_cursor`. A zero
    // here would mean the tally never fired rather than that the page was empty.
    try std.testing.expectEqual(r.body.len, measured.encoded_bytes);
    try std.testing.expect(measured.encoded_bytes > 0);
}
