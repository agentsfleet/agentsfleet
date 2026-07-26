//! Integration tier for §3's body ceiling and the input bound that makes it
//! unreachable — the `UZ-LIBRARY-005` half of `test_library_read_resource_bounds`.
//!
//! Split from `library_read_bounds_integration_test.zig` when that file crossed
//! the 350-line cap (RULE FLL). The seam is the question, not the line count:
//! that file measures what a compliant read COSTS, this one proves what an
//! over-ceiling response DOES — refuse, with a typed code, never a truncated
//! page — and that no API input can reach the ceiling in the first place.
//!
//! `LIVE_DB=1` + `TEST_DATABASE_URL` (set by `make test-integration`);
//! self-skips otherwise.

const std = @import("std");

const base = @import("secrets_json_integration_test.zig");
const fixtures = @import("library_bounds_test_fixtures.zig");
const harness_mod = @import("test_harness.zig");
const ec = @import("../errors/error_registry.zig");
const counters = @import("../observability/library_read_counters.zig");
const model_identity = @import("../types/model_identity.zig");

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
    // DEFERRED, unlike the budget suite's tests, and the difference matters.
    // The rows below make the shared tenant's Models page exceed its ceiling —
    // which is the point — but that page is read by sibling suites that clean
    // only on their own way out. If this test failed between the seed and a
    // trailing cleanup, those rows would survive and every later GET of that
    // page would 500, reporting the failure against whichever sibling ran next
    // rather than against the test that caused it.
    defer fixtures.cleanup(h, "oversize");
    try fixtures.seedEntries(alloc, h);

    // Restored unconditionally: a leaked override would make every later test's
    // page refuse, and the failures would point at those tests rather than here.
    counters.setTenantRegistryBodyCeilingForTest(TEST_BODY_CEILING_BYTES);
    defer counters.setTenantRegistryBodyCeilingForTest(null);

    const r = try (try h.get(fixtures.MODELS_PATH).bearer(base.TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.internal_server_error);
    try r.expectErrorCode(ec.ERR_LIBRARY_BODY_CEILING);

    // Refused, not truncated: no page escaped alongside the error. A handler
    // that wrote a shortened `models` array AND an error would satisfy the
    // status assertion above while shipping exactly the silent data loss §3
    // forbids.
    try std.testing.expect(!r.bodyContains("\"models\""));

    // And the byte tally stays unrecorded for a body that was never sent, so a
    // refused page cannot inflate the measurement the budget suite asserts on.
    try std.testing.expectEqual(@as(usize, 0), counters.snapshot().encoded_bytes);
}

/// POST a `model_id` of exactly `len` bytes, returning the status.
fn postModelId(alloc: std.mem.Allocator, h: *harness_mod.TestHarness, len: usize) !u16 {
    const id = try alloc.alloc(u8, len);
    defer alloc.free(id);
    @memset(id, 'm');
    const body = try std.fmt.allocPrint(
        alloc,
        "{{\"model_id\":\"{s}\",\"secret_ref\":\"" ++ fixtures.SECRET_NAME ++ "\"}}",
        .{id},
    );
    defer alloc.free(body);
    const r = try (try (try h.post(fixtures.MODELS_PATH).bearer(base.TOKEN_OPERATOR)).json(body)).send();
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
    defer fixtures.cleanup(h, "bound-test");
    try fixtures.seedCredential(alloc, h, "sk-ant-bound-probe");

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
