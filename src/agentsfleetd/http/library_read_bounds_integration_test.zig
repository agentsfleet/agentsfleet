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
//! `statements == 5` fails when the read grows a sixth. `statements <= 5` also
//! fails then, but an exact count additionally fails when a statement is
//! REMOVED — which is how a page stops resolving the platform default and
//! nobody notices until the Default row renders empty in production.
//!
//! `LIVE_DB=1` + `TEST_DATABASE_URL` (set by `make test-integration`);
//! self-skips otherwise.

const std = @import("std");

const base = @import("secrets_json_integration_test.zig");
const harness_mod = @import("test_harness.zig");
const ec = @import("../errors/error_registry.zig");
const counters = @import("../observability/library_read_counters.zig");
const crypto_store = @import("../secrets/crypto_store.zig");
const model_identity = @import("../types/model_identity.zig");

const MODELS_PATH = "/v1/tenants/me/models";

/// One credential backing every seeded entry. The page is a metadata read, so
/// which credential the rows name does not matter — only that one EXISTS, which
/// keeps the projection on its normal path instead of the degraded
/// `custom_secret` branch that skips the metadata batch's interesting half.
const SECRET_NAME = "bounds-probe-key";

/// Three entries and a page of two: enough that the page is genuinely truncated
/// (so `next_cursor` is produced and the result tally is bounded by `limit`
/// rather than by how many rows happen to exist), and few enough that seeding
/// stays three requests.
const SEEDED_ENTRIES: usize = 3;
const SMALL_PAGE: usize = 2;

/// The tenant registry page's five statements, enumerated so a failure of the
/// exact-count assertion can be read against the list rather than rediscovered:
///
///   1. `tenant_provider.activeSelfManagedRef`  — which entry is active
///   2. `entries_state.listPage`                — the page itself (over-fetch by one)
///   3. `secret_probe.resolvePrimaryWorkspace`  — once for the page, not per row
///   4. `vault.loadMetadata`                    — one batch for every row's metadata
///   5. `tenant_provider.platformDefaultView`   — the Default row's identity
///
/// Independent of page size by construction: 3 and 4 are the two that a naive
/// per-row projection would multiply, and both are hoisted.
const EXPECTED_STATEMENTS: usize = 5;

fn seedEntries(alloc: std.mem.Allocator, h: *harness_mod.TestHarness) !void {
    const secrets_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets", .{base.TEST_WS_ID});
    defer alloc.free(secrets_path);
    {
        const r = try (try (try h.post(secrets_path).bearer(base.TOKEN_OPERATOR))
            .json("{\"name\":\"" ++ SECRET_NAME ++ "\",\"data\":{\"provider\":\"anthropic\",\"api_key\":\"sk-ant-bounds-probe\"}}")).send();
        defer r.deinit();
        try r.expectStatus(.created);
    }

    var n: usize = 0;
    while (n < SEEDED_ENTRIES) : (n += 1) {
        const body = try std.fmt.allocPrint(
            alloc,
            "{{\"model_id\":\"claude-bounds-probe-{d}\",\"secret_ref\":\"" ++ SECRET_NAME ++ "\"}}",
            .{n},
        );
        defer alloc.free(body);
        const r = try (try (try h.post(MODELS_PATH).bearer(base.TOKEN_OPERATOR)).json(body)).send();
        defer r.deinit();
        try r.expectStatus(.created);
    }
}

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
    }
    try seedEntries(alloc, h);

    // ── a truncated page: the tally is bounded by `limit`, not by row count ──
    {
        const path = std.fmt.comptimePrint("{s}?limit={d}", .{ MODELS_PATH, SMALL_PAGE });
        crypto_store.resetDecryptCountForTest();
        const r = try (try h.get(path).bearer(base.TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.ok);

        // The window closed when the handler returned, so this snapshot is that
        // request's and nothing else's.
        try expectRegistryBudget(counters.snapshot(), r.body.len, SMALL_PAGE);

        // The page really was short — otherwise `results == SMALL_PAGE` could
        // mean the seed failed rather than that the limit held.
        try std.testing.expect(!r.bodyContains("\"next_cursor\":null"));
    }

    // ── the full page: same five statements, more rows, still zero decrypts ──
    {
        crypto_store.resetDecryptCountForTest();
        const r = try (try h.get(MODELS_PATH).bearer(base.TOKEN_OPERATOR)).send();
        defer r.deinit();
        try r.expectStatus(.ok);

        // The budget does not scale with page size. That is the whole claim of
        // §1's projection rewrite, and it is the one a per-row read would break
        // here while still passing every assertion in the block above.
        try expectRegistryBudget(counters.snapshot(), r.body.len, SEEDED_ENTRIES);
        try std.testing.expect(r.bodyContains("\"next_cursor\":null"));
    }

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    base.cleanupRows(conn);
}

/// A ceiling small enough that the seeded page overruns it, and large enough
/// that it is clearly a ceiling rather than zero.
///
/// This test used to reach the REAL 512 KiB ceiling by planting three 200 KB
/// `model_id` values — which worked, and was the reproduction that justified
/// bounding the field. With `model_id` capped at 256 bytes
/// (`types/model_identity.zig`) that route is closed on purpose: a full page of
/// maximal rows is ~66 KB, so no API input can breach the real ceiling any
/// more. Lowering the ceiling is what keeps the error contract under test after
/// the input that could trigger it stopped existing. The real constant is
/// pinned by a unit test; what this proves is the mapping from "over ceiling"
/// to `UZ-LIBRARY-005`, which is the part only the handler can get wrong.
const TEST_BODY_CEILING_BYTES: usize = 256;

test "integration: test_library_read_resource_bounds — an over-ceiling page is refused with UZ-LIBRARY-005, never truncated" {
    // The error contract for the ceiling, end to end. The unit tier proves the
    // RULE (`response_size.encodedWithinCeiling`, including the exactly-at-the-
    // ceiling boundary); this proves the handler maps that refusal onto the
    // right status and error code instead of, say, a bare 500 or a short 200.
    //
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
    }
    // DEFERRED, unlike this file's other tests, and the difference matters.
    // The rows below make the shared tenant's Models page exceed its ceiling —
    // which is the point — but that page is read by sibling suites that clean
    // only on their own way out. If this test failed between the seed and a
    // trailing cleanup, those rows would survive and every later GET of that
    // page would 500, reporting the failure against whichever sibling ran next
    // rather than against the test that caused it.
    defer {
        if (h.acquireConn()) |conn| {
            base.cleanupRows(conn);
            h.releaseConn(conn);
        } else |err| {
            std.log.warn("oversize cleanup skipped: {s}", .{@errorName(err)});
        }
    }
    try seedEntries(alloc, h);

    // Restored unconditionally: a leaked override would make every later test's
    // page refuse, and the failures would point at those tests rather than here.
    counters.setTenantRegistryBodyCeilingForTest(TEST_BODY_CEILING_BYTES);
    defer counters.setTenantRegistryBodyCeilingForTest(null);

    const r = try (try h.get(MODELS_PATH).bearer(base.TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.internal_server_error);
    try r.expectErrorCode(ec.ERR_LIBRARY_BODY_CEILING);

    // Refused, not truncated: no page escaped alongside the error. A handler
    // that wrote a shortened `models` array AND an error would satisfy the
    // status assertion above while shipping exactly the silent data loss §3
    // forbids.
    try std.testing.expect(!r.bodyContains("\"models\""));

    // And the byte tally stays unrecorded for a body that was never sent, so a
    // refused page cannot inflate the measurement the other tests assert on.
    try std.testing.expectEqual(@as(usize, 0), counters.snapshot().encoded_bytes);
}

/// POST a `model_id` of exactly `len` bytes, returning the status.
fn postModelId(alloc: std.mem.Allocator, h: *harness_mod.TestHarness, len: usize) !u16 {
    const id = try alloc.alloc(u8, len);
    defer alloc.free(id);
    @memset(id, 'm');
    const body = try std.fmt.allocPrint(
        alloc,
        "{{\"model_id\":\"{s}\",\"secret_ref\":\"" ++ SECRET_NAME ++ "\"}}",
        .{id},
    );
    defer alloc.free(body);
    const r = try (try (try h.post(MODELS_PATH).bearer(base.TOKEN_OPERATOR)).json(body)).send();
    defer r.deinit();
    return r.status;
}

test "integration: test_library_read_resource_bounds — model_id is bounded at the write, so the page cannot be made unreadable" {
    // The regression guard for the hazard that motivated the bound. Before it,
    // three ~200 KB model_ids (compressible enough to fit the unique index, and
    // they DID insert) pushed the tenant's own Models page past its ceiling —
    // permanently, since the page is how you find the rows to delete them. The
    // same rows also made every projected row hash a 200 KB key under the
    // process-global rate-cache mutex that billing shares, so the blast radius
    // reached other tenants.
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
    }
    defer {
        if (h.acquireConn()) |conn| {
            base.cleanupRows(conn);
            h.releaseConn(conn);
        } else |err| {
            std.log.warn("bound-test cleanup skipped: {s}", .{@errorName(err)});
        }
    }
    {
        const secrets_path = try std.fmt.allocPrint(alloc, "/v1/workspaces/{s}/secrets", .{base.TEST_WS_ID});
        defer alloc.free(secrets_path);
        const r = try (try (try h.post(secrets_path).bearer(base.TOKEN_OPERATOR))
            .json("{\"name\":\"" ++ SECRET_NAME ++ "\",\"data\":{\"provider\":\"anthropic\",\"api_key\":\"sk-ant-bound-probe\"}}")).send();
        defer r.deinit();
        try r.expectStatus(.created);
    }

    // Exactly at the bound is ACCEPTED. Asserted first and separately: a bound
    // that rejects its own maximum is an outage for whoever ships a 256-byte
    // model name, and no over-the-limit test can see that.
    try std.testing.expectEqual(
        @as(u16, 201),
        try postModelId(alloc, h, model_identity.MODEL_ID_MAX),
    );

    // One byte over is refused with 400 — a client input fault reported AS one.
    // Past the index limit Postgres used to raise an index-size error that the
    // handler surfaced as `503 Database unavailable`, which pointed at the
    // database instead of at the request.
    try std.testing.expectEqual(
        @as(u16, 400),
        try postModelId(alloc, h, model_identity.MODEL_ID_MAX + 1),
    );

    // And the size that used to brick the page is now refused outright, rather
    // than accepted and discovered on the next read.
    try std.testing.expectEqual(@as(u16, 400), try postModelId(alloc, h, 200_000));
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
    }

    crypto_store.resetDecryptCountForTest();
    const r = try (try h.get(MODELS_PATH).bearer(base.TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);

    const measured = counters.snapshot();

    // Four, not five: `vault.loadMetadata` returns before querying when the
    // page has no rows to describe. Pinned rather than waved through as "under
    // the ceiling" because the degenerate page is where a guard clause gets
    // deleted during a refactor and nobody notices the extra statement —
    // it is, after all, still under budget.
    try std.testing.expectEqual(EXPECTED_STATEMENTS - 1, measured.statements);
    try std.testing.expectEqual(@as(usize, 0), measured.results);
    try std.testing.expectEqual(@as(usize, 0), crypto_store.decryptCountForTest());
    try std.testing.expectEqual(counters.MAX_CONNECTIONS_PER_READ, measured.connections);

    // An empty page is still a body — envelope, `total`, `next_cursor`. A zero
    // here would mean the tally never fired rather than that the page was empty.
    try std.testing.expectEqual(r.body.len, measured.encoded_bytes);
    try std.testing.expect(measured.encoded_bytes > 0);
}
