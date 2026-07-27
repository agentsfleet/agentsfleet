//! Integration tier for §2 Dimension 2.1 — `test_library_deterministic_resource_gate`.
//!
//! ## What this adds over the resource-bounds suite next door
//!
//! `library_read_bounds_integration_test.zig` already proves the tenant
//! registry page stays inside its numeric table, decryptions included. This
//! file proves the STAGE ATTRIBUTION of the same read: that the work the table
//! counts is recorded against the stages that did it, that exactly one terminal
//! outcome closes the request, and — the claim §2 argues for at length — that
//! `secret_project` both RAN and spent zero decryptions.
//!
//! Those are different failures. A read can stay inside every ceiling while
//! recording its cost against the wrong stage, or against no stage at all, and
//! the ceiling suite would pass throughout. An operator reading a flat
//! `secret_project` would then conclude presence resolution is free, when in
//! fact the stage stopped being recorded.
//!
//! ## Why `secret_project` at zero decryptions rather than a deleted label
//!
//! The read-path decryption is gone: `vault.loadMetadata` answers one batch
//! presence query and opens no envelope. Deleting the stage with the decryption
//! would have been the tidy move and the wrong one — a regression that
//! reintroduces per-row decryption would then show up as a stage that silently
//! REAPPEARS, which nothing is watching for. Keeping it and pinning its
//! decryption count at zero makes that regression show up as a stage that
//! suddenly decrypts, which this test fails on.
//!
//! Every maximum below is IMPORTED from `library_read_counters.zig`, never
//! retyped. That module is the one home for the table (Invariant 2), and a
//! ceiling relaxed there is visible in its own pin test; a ceiling retyped here
//! would let this file and that one disagree silently.
//!
//! `LIVE_DB=1` + `TEST_DATABASE_URL` (set by `make test-integration`);
//! self-skips otherwise.

const std = @import("std");

const base = @import("secrets_json_integration_test.zig");
const fixtures = @import("library_bounds_test_fixtures.zig");
const counters = @import("../observability/library_read_counters.zig");
const crypto_store = @import("../secrets/crypto_store.zig");
const fixtures_provider = @import("../db/test_fixtures_provider.zig");
const stages = @import("../observability/library_stages.zig");

/// Stages the tenant registry read must record on its success path, with the
/// EXACT number of observations each makes. Named as a table rather than
/// asserted inline so a stage that stops firing is reported by name instead of
/// as a bare count mismatch.
///
/// `sql` is 2, and that is not a rounding of "about one". The read issues its
/// three page statements, then resolves secret presence, then issues the
/// platform default and the rate batch — so SQL happens on both sides of
/// `secret_project` and is recorded as two spans of one read. Collapsing them
/// would mean either timing the presence query as SQL (losing the distinction
/// the stage exists for) or accumulating a partial span across an unrelated
/// stage (making the marker stateful for one caller's convenience).
///
/// This is exactly why the exposition carries `stage_observations_total` as its
/// own family rather than reusing the per-request outcome count as a
/// denominator: `rate(duration)/rate(observations)` is the mean cost of a SPAN,
/// which is well-defined whether a stage fires once or twice, while
/// duration-over-requests would silently halve this stage's apparent cost.
const REGISTRY_STAGES = [_]struct { stage: stages.Stage, times: u64 }{
    .{ .stage = .auth_verify, .times = 1 },
    .{ .stage = .pool_wait, .times = 1 },
    .{ .stage = .sql, .times = 2 },
    .{ .stage = .secret_project, .times = 1 },
    .{ .stage = .map, .times = 1 },
    .{ .stage = .serialize, .times = 1 },
};

/// Stages this surface cannot reach. Asserting their absence is what keeps the
/// success-path assertion from passing against a recorder that fires
/// everything — a table where every cell moved would satisfy any
/// "these stages ran" check.
const REGISTRY_NON_STAGES = [_]stages.Stage{
    .cache_lookup,
    .cache_revision,
    .authorize,
    .next_upstream,
};

fn stageCount(snap: stages.Snapshot, stage: stages.Stage) u64 {
    return snap.stages[@intFromEnum(stages.Surface.tenant_models)][@intFromEnum(stage)].count;
}

fn outcomeCount(snap: stages.Snapshot, outcome: stages.Outcome) u64 {
    return snap.read_outcomes[@intFromEnum(stages.Surface.tenant_models)][@intFromEnum(outcome)];
}

fn totalOutcomes(snap: stages.Snapshot) u64 {
    var total: u64 = 0;
    for (snap.read_outcomes) |row| for (row) |v| {
        total += v;
    };
    return total;
}

test "integration: test_library_deterministic_resource_gate — the registry read attributes its cost to stages and decrypts nothing" {
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
        // The rate batch only runs with a pair to resolve, and it is one of the
        // statements the budget counts, so this suite owns that state rather
        // than inheriting whatever a sibling left behind.
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

    stages.resetForTest();
    crypto_store.resetDecryptCountForTest();

    const path = std.fmt.comptimePrint("{s}?limit={d}", .{ fixtures.MODELS_PATH, fixtures.SMALL_PAGE });
    const r = try (try h.get(path).bearer(base.TOKEN_OPERATOR)).send();
    defer r.deinit();
    try r.expectStatus(.ok);

    const measured = counters.snapshot();
    const snap = stages.snapshot();

    // ── the numeric table, consumed from its owning module ──────────────────
    try std.testing.expect(measured.statements <= counters.TENANT_REGISTRY_MAX_STATEMENTS);
    try std.testing.expect(measured.results <= counters.TENANT_REGISTRY_MAX_RESULTS);
    try std.testing.expect(measured.encoded_bytes <= counters.TENANT_REGISTRY_MAX_BODY_BYTES);
    try std.testing.expectEqual(counters.MAX_CONNECTIONS_PER_READ, measured.connections);
    // Zero read-path decryptions, the invariant the whole `secret_project`
    // argument rests on.
    try std.testing.expectEqual(@as(usize, 0), crypto_store.decryptCountForTest());

    // Non-vacuous: a request that never reached the handler would satisfy every
    // `<=` above.
    try std.testing.expect(measured.statements > 0);
    try std.testing.expect(measured.encoded_bytes > 0);

    // ── the stage attribution ───────────────────────────────────────────────
    for (REGISTRY_STAGES) |expected| {
        const actual = stageCount(snap, expected.stage);
        if (actual != expected.times) {
            std.debug.print(
                "stage '{s}' recorded {d} observations, expected exactly {d}\n",
                .{ @tagName(expected.stage), actual, expected.times },
            );
            return error.StageNotRecordedExpectedTimes;
        }
    }
    for (REGISTRY_NON_STAGES) |stage| {
        if (stageCount(snap, stage) != 0) {
            std.debug.print(
                "stage '{s}' fired on a surface that cannot reach it ({d} observations)\n",
                .{ @tagName(stage), stageCount(snap, stage) },
            );
            return error.UnreachableStageRecorded;
        }
    }

    // `secret_project` ran AND cost nothing to decrypt. The two halves together
    // are the claim; either alone is satisfiable by a regression.
    try std.testing.expectEqual(@as(u64, 1), stageCount(snap, .secret_project));
    try std.testing.expectEqual(@as(usize, 0), crypto_store.decryptCountForTest());

    // ── exactly one terminal outcome, and it is the served page ─────────────
    try std.testing.expectEqual(@as(u64, 1), totalOutcomes(snap));
    try std.testing.expectEqual(@as(u64, 1), outcomeCount(snap, .ok));

    // The connection it took is accounted for once, on the stage that took it.
    try std.testing.expectEqual(
        @as(u64, 1),
        snap.pool_results[@intFromEnum(stages.PoolResult.acquired)],
    );

    // Body bytes and row count reached the evidence tables, not just the
    // test-only counters — §4's report reads these, so a stage that recorded
    // its duration but dropped its payload would produce an aggregate with a
    // permanently zero `payload_bytes`.
    const s = @intFromEnum(stages.Surface.tenant_models);
    try std.testing.expect(snap.payload_bytes[s] > 0);
    try std.testing.expectEqual(@as(u64, fixtures.SMALL_PAGE), snap.results[s]);

    stages.resetForTest();
}

test "integration: test_library_deterministic_resource_gate — a rejected request still reports exactly one outcome" {
    base.setTestEncryptionKey();
    const alloc = std.testing.allocator;
    const h = base.seedAndHarness(alloc) catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    stages.resetForTest();

    // An out-of-range limit is refused before a connection is acquired — the
    // cheapest rejection the handler has, and the one most likely to be missed
    // by instrumentation placed around the database work.
    const r = try (try h.get(fixtures.MODELS_PATH ++ "?limit=99999").bearer(base.TOKEN_OPERATOR)).send();
    defer r.deinit();

    const snap = stages.snapshot();
    // Still exactly one. A read rejected at the door is still a read an
    // operator's rate needs to count.
    try std.testing.expectEqual(@as(u64, 1), totalOutcomes(snap));
    try std.testing.expectEqual(@as(u64, 1), outcomeCount(snap, .invalid));
    try std.testing.expectEqual(@as(u64, 0), outcomeCount(snap, .ok));

    // No pool slot was spent, so no pool result was recorded — the rejection
    // really did happen before the acquire rather than after it.
    for (snap.pool_results) |v| try std.testing.expectEqual(@as(u64, 0), v);

    stages.resetForTest();
}
