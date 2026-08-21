//! Allocation-failure proofs for `tenant_provider_resolver.zig`.
//!
//! The resolver is the read path that turns a tenant's stored selection into a
//! fully-populated `ResolvedProvider`, api_key included. Four of its entry
//! points build that value one `alloc.dupe` at a time, each dupe guarded by an
//! `errdefer` that frees the dupes before it — thirteen rungs across the file,
//! none of which an ordinary green test touches, because every one of them runs
//! only when a LATER allocation fails.
//!
//! Two of those rungs also `secureZero` the api_key before freeing it. A rung
//! that never runs is a plaintext credential left in a freed heap block, so
//! "the ladder looks right" is not a standard this file can be held to.
//!
//! `checkAllAllocationFailures` fails each allocation site in turn and asserts
//! the call both propagated `error.OutOfMemory` and leaked nothing on the way
//! out. That is exhaustive over sites and identical on every machine.
//!
//! Fixtures come from `tenant_provider_test.zig`, which already exports the
//! platform-key and self-managed seeds these reads need; the proofs live here
//! rather than there because that file is at its length cap.

const std = @import("std");
const pg = @import("pg");

const resolver = @import("tenant_provider_resolver.zig");
const tenant_provider = @import("tenant_provider.zig");
const base = @import("../db/test_fixtures.zig");
const uc1 = @import("../db/test_fixtures_uc1.zig");
const fixture = @import("tenant_provider_test.zig");

const ALLOC = std.testing.allocator;

/// Workspaces of this file's own, so a proof cannot collide with the sibling
/// suites that share `uc1.TENANT_ID`.
const WS_ALLOC_PLATFORM = "0195b4ba-8d3a-7f13-8abc-aa2000000004";
const WS_ALLOC_SELF_MANAGED = "0195b4ba-8d3a-7f13-8abc-aa2000000005";

const SELF_MANAGED_REF = "account-alloc-proof";
const SELF_MANAGED_MODEL = "accounts/fireworks/models/alloc-proof";
const SELF_MANAGED_KEY = "fw_ALLOC_PROOF_key";

// ── Wrappers ──────────────────────────────────────────────────────────────
// `checkAllAllocationFailures` requires `fn(Allocator, ...) !void`, so each
// entry point gets a wrapper that calls it and frees what it returned. The
// wrapper frees on the SUCCESS path only: on an induced failure the function
// under test owns the unwind, which is the whole point of the proof.

fn loadProviderRowUnderAllocator(alloc: std.mem.Allocator, conn: *pg.Conn) !void {
    var row = (try resolver.loadProviderRow(alloc, conn, uc1.TENANT_ID)) orelse
        return error.FixtureRowMissing;
    row.deinit(alloc);
}

fn loadActivePlatformKeyUnderAllocator(alloc: std.mem.Allocator, conn: *pg.Conn) !void {
    var plk = try resolver.loadActivePlatformKey(alloc, conn);
    plk.deinit(alloc);
}

fn resolvePlatformDefaultUnderAllocator(alloc: std.mem.Allocator, conn: *pg.Conn) !void {
    var rp = try resolver.resolvePlatformDefault(alloc, conn);
    rp.deinit(alloc);
}

/// Takes the row by value: `resolveSelfManaged` reads `secret_ref` and `model`
/// off it and never frees them, so the row is loaded once by the caller on the
/// backing allocator and stays valid across every induced-failure run.
fn resolveSelfManagedUnderAllocator(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    row: resolver.ProviderRow,
) !void {
    var rp = try resolver.resolveSelfManaged(alloc, conn, uc1.TENANT_ID, row);
    rp.deinit(alloc);
}

// ── Proofs ────────────────────────────────────────────────────────────────

test "every allocation site in the tenant selection read unwinds without leaking" {
    fixture.setEncryptionKey();
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_ALLOC_SELF_MANAGED);
    defer fixture.cleanupTeardown(db_ctx.conn, WS_ALLOC_SELF_MANAGED);

    try fixture.seedSelfManagedCredential(
        db_ctx.conn,
        ALLOC,
        WS_ALLOC_SELF_MANAGED,
        SELF_MANAGED_REF,
        fixture.TP_TEST_PROVIDER,
        SELF_MANAGED_KEY,
        SELF_MANAGED_MODEL,
    );
    // A self-managed selection carries a non-null secret_ref, so this drives the
    // optional-dupe branch a platform row would leave null.
    try tenant_provider.upsertSelfManaged(
        ALLOC,
        db_ctx.conn,
        uc1.TENANT_ID,
        SELF_MANAGED_REF,
        SELF_MANAGED_MODEL,
        256_000,
    );

    try std.testing.checkAllAllocationFailures(
        ALLOC,
        loadProviderRowUnderAllocator,
        .{db_ctx.conn},
    );
}

test "every allocation site in the active platform key read unwinds without leaking" {
    fixture.setEncryptionKey();
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_ALLOC_PLATFORM);
    defer fixture.cleanupTeardown(db_ctx.conn, WS_ALLOC_PLATFORM);

    try fixture.seedPlatformLlmKey(
        db_ctx.conn,
        ALLOC,
        WS_ALLOC_PLATFORM,
        fixture.TP_TEST_PROVIDER,
        "fw_PLATFORM_alloc_proof",
    );

    try std.testing.checkAllAllocationFailures(
        ALLOC,
        loadActivePlatformKeyUnderAllocator,
        .{db_ctx.conn},
    );
}

test "every allocation site in the platform resolve unwinds without leaking the api key" {
    fixture.setEncryptionKey();
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_ALLOC_PLATFORM);
    defer fixture.cleanupTeardown(db_ctx.conn, WS_ALLOC_PLATFORM);

    try fixture.seedPlatformLlmKey(
        db_ctx.conn,
        ALLOC,
        WS_ALLOC_PLATFORM,
        fixture.TP_TEST_PROVIDER,
        "fw_PLATFORM_alloc_proof",
    );

    // This one covers the secureZero-then-free rung: the api_key is fetched
    // before provider and model are duped, so a failure at either of those two
    // sites is what runs it.
    try std.testing.checkAllAllocationFailures(
        ALLOC,
        resolvePlatformDefaultUnderAllocator,
        .{db_ctx.conn},
    );
}

test "every allocation site in the self-managed resolve unwinds without leaking the api key" {
    fixture.setEncryptionKey();
    const db_ctx = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer db_ctx.pool.deinit();
    defer db_ctx.pool.release(db_ctx.conn);

    try uc1.seed(db_ctx.conn, WS_ALLOC_SELF_MANAGED);
    defer fixture.cleanupTeardown(db_ctx.conn, WS_ALLOC_SELF_MANAGED);

    try fixture.seedSelfManagedCredential(
        db_ctx.conn,
        ALLOC,
        WS_ALLOC_SELF_MANAGED,
        SELF_MANAGED_REF,
        fixture.TP_TEST_PROVIDER,
        SELF_MANAGED_KEY,
        SELF_MANAGED_MODEL,
    );
    try tenant_provider.upsertSelfManaged(
        ALLOC,
        db_ctx.conn,
        uc1.TENANT_ID,
        SELF_MANAGED_REF,
        SELF_MANAGED_MODEL,
        256_000,
    );

    var row = (try resolver.loadProviderRow(ALLOC, db_ctx.conn, uc1.TENANT_ID)) orelse
        return error.FixtureRowMissing;
    defer row.deinit(ALLOC);

    try std.testing.checkAllAllocationFailures(
        ALLOC,
        resolveSelfManagedUnderAllocator,
        .{ db_ctx.conn, row },
    );
}
