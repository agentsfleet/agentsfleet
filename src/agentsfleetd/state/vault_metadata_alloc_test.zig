//! Allocation-failure proofs for `vault.zig`'s metadata read.
//!
//! `loadMetadata` is the one read in this file that builds owned memory behind
//! a ladder. It fills a caller-supplied `out` slice one `rowToMetadata` at a
//! time, and three `errdefer` rungs guard the way out: the outer rung releases
//! every slot already filled, and two inner rungs release `provider` and
//! `base_url` when a later dupe fails part-way through a single projection.
//! None of the three runs on an ordinary green read — every one of them runs
//! only when a LATER allocation fails.
//!
//! Two things the fixture has to get right, or the proof passes while proving
//! nothing:
//!
//! 1. **The inner rungs are OPTIONAL.** `provider` and `base_url` are only
//!    allocation sites when their columns are non-null, so a row seeded without
//!    them makes the rungs unreachable and the proof decorative. Both rows here
//!    carry a `provider` AND a `base_url`, which is what an openai-compatible
//!    credential projects to.
//! 2. **The outer rung needs a filled slot behind it.** With one row there is
//!    nothing for `freeMetadata` to release, so the rung is reached and does
//!    nothing observable. Two rows means a failure in the second projection
//!    unwinds a first that already owns strings.
//!
//! `checkAllAllocationFailures` fails each allocation site in turn and asserts
//! the call both propagated `error.OutOfMemory` and leaked nothing on the way
//! out — exhaustive over sites, identical on every machine. `loadMetadata` only
//! reads, so no run of it commits and none of them needs a per-run reset.

const std = @import("std");
const pg = @import("pg");

const vault = @import("vault.zig");
const base = @import("../db/test_fixtures.zig");
const crypto_primitives = @import("../secrets/crypto_primitives.zig");

const ALLOC = std.testing.allocator;

/// A workspace of this file's own, so a proof cannot collide with the sibling
/// suites that share `vault_test.zig`'s workspace.
const WS_ALLOC_METADATA = "0195b4ba-8d3a-7f13-8abc-cd0000000009";

const KEY_FIRST = "fleet:alloc-proof-first";
const KEY_SECOND = "fleet:alloc-proof-second";

/// Both bodies project to a `custom_endpoint` with BOTH promoted string columns
/// non-null, which is what makes the two inner rungs allocation sites at all.
const PROVIDER_OPENAI_COMPATIBLE = "openai-compatible";
const BASE_URL_FIRST = "https://first.alloc-proof.invalid/v1";
const BASE_URL_SECOND = "https://second.alloc-proof.invalid/v1";
const API_KEY_FIELD = "api_key";
const PROVIDER_FIELD = "provider";
const BASE_URL_FIELD = "base_url";
const API_KEY_VALUE = "sk-alloc-proof-not-a-real-key";

const CANDIDATE_COUNT = 2;

fn buildEndpointCredential(alloc: std.mem.Allocator, base_url: []const u8) !std.json.Value {
    var obj: std.json.ObjectMap = .empty;
    try obj.put(alloc, PROVIDER_FIELD, .{ .string = PROVIDER_OPENAI_COMPATIBLE });
    try obj.put(alloc, BASE_URL_FIELD, .{ .string = base_url });
    try obj.put(alloc, API_KEY_FIELD, .{ .string = API_KEY_VALUE });
    return .{ .object = obj };
}

fn seedBothRows(alloc: std.mem.Allocator, conn: *pg.Conn) !void {
    try base.seedTenant(conn);
    try base.seedWorkspace(conn, WS_ALLOC_METADATA);

    var first = try buildEndpointCredential(alloc, BASE_URL_FIRST);
    defer first.object.deinit(alloc);
    try base.storeVaultJson(alloc, conn, WS_ALLOC_METADATA, KEY_FIRST, first);

    var second = try buildEndpointCredential(alloc, BASE_URL_SECOND);
    defer second.object.deinit(alloc);
    try base.storeVaultJson(alloc, conn, WS_ALLOC_METADATA, KEY_SECOND, second);
}

fn cleanupRows(conn: *pg.Conn) void {
    _ = conn.exec(
        "DELETE FROM vault.secrets WHERE workspace_id = $1",
        .{WS_ALLOC_METADATA},
    ) catch |err| std.log.warn("ignored: {s}", .{@errorName(err)});
    base.teardownWorkspace(conn, WS_ALLOC_METADATA);
    base.teardownTenant(conn);
}

// ── Wrapper ───────────────────────────────────────────────────────────────
// `checkAllAllocationFailures` requires `fn(Allocator, ...) !void`, so the read
// gets a wrapper that calls it and frees what it filled. The wrapper frees on
// the SUCCESS path only: on an induced failure `loadMetadata` owns the unwind,
// which is the whole point of the proof.

fn loadMetadataUnderAllocator(alloc: std.mem.Allocator, conn: *pg.Conn) !void {
    const candidates = [CANDIDATE_COUNT][]const u8{ KEY_FIRST, KEY_SECOND };
    var out: [CANDIDATE_COUNT]?vault.SecretMetadata = undefined;
    try vault.loadMetadata(alloc, conn, WS_ALLOC_METADATA, &candidates, &out);
    defer vault.freeMetadata(alloc, &out);

    // The fixture is the proof for the optional rungs: if either promoted
    // column came back null, the two inner rungs were never allocation sites
    // and a green run below would mean nothing.
    for (out) |slot| {
        const m = slot orelse return error.FixtureRowMissing;
        _ = m.provider orelse return error.FixtureProviderNull;
        _ = m.base_url orelse return error.FixtureBaseUrlNull;
    }
}

// ── Proof ─────────────────────────────────────────────────────────────────

test "every allocation site in the vault metadata read unwinds without leaking" {
    crypto_primitives.setTestKek();
    const handle = (try base.openTestConn(ALLOC)) orelse return error.SkipZigTest;
    defer {
        handle.pool.release(handle.conn);
        handle.pool.deinit();
    }
    try seedBothRows(ALLOC, handle.conn);
    defer cleanupRows(handle.conn);

    try std.testing.checkAllAllocationFailures(
        ALLOC,
        loadMetadataUnderAllocator,
        .{handle.conn},
    );
}
