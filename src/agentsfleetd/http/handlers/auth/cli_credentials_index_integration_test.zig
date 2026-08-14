//! What the DATABASE refuses, and what the authenticate path never writes.
//!
//! The rule "one live credential per machine" is held by a partial unique index
//! rather than by store discipline, precisely so it cannot be skipped by a code
//! path that forgot. An assertion against the statement text cannot prove that
//! — only an insert the datastore actually refuses can. Likewise, "attribution
//! is written once at mint and never on the authenticate path" is a claim about
//! what does NOT happen across many requests, which no unit test can observe.
//!
//! Requires TEST_DATABASE_URL — skipped gracefully otherwise.

const std = @import("std");

const pg = @import("pg");

const fixtures = @import("cli_credentials_test_fixtures.zig");
const clock = @import("common").clock;
const ec = @import("../../../errors/error_registry.zig");
const store = @import("../../../state/cli_credentials.zig");

const ALLOC = fixtures.ALLOC;
const PATH = fixtures.PATH;

/// Authenticated reads issued between the two row snapshots. Large enough that
/// a per-request write — a usage stamp, a counter, a touched timestamp — shows
/// up as a changed column rather than hiding inside timing noise.
const AUTH_REQUESTS = 100;

/// Logins raced against one another on a single `(user, machine)`.
///
/// Not the harness's ceiling, and deliberately so: every caller here contends
/// for the SAME single index entry, so contention is already total at this
/// count and further callers only queue against the server's worker pool
/// (2 threads, 2 workers) rather than sharpening the race under test.
const CONCURRENT_LOGINS = 16;

/// A syntactically valid version-7 identifier, for the row the index must
/// refuse. Fixed rather than generated so a failure names a constant.
const SECOND_ROW_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0f7031";

/// Two distinct stand-in digests. Distinct on purpose: reusing one would let
/// the UNIQUE constraint on `credential_hash` refuse the second insert, and the
/// test would pass while proving nothing about the partial index it names.
const REFUSED_HASH = "a" ** 64;
const ACCEPTED_HASH = "b" ** 64;

const INSERT_RAW_CREDENTIAL =
    \\INSERT INTO core.cli_credentials
    \\  (id, user_id, tenant_id, machine_name, credential_hash, credential_prefix,
    \\   deployment, created_from_address, created_at)
    \\VALUES ($1::uuid, $2::uuid, $3::uuid, $4, $5, 'afc_deadbeef',
    \\        'http://127.0.0.1:0', '127.0.0.1', $6::bigint)
;

test "integration: test_second_live_credential_per_machine_is_refused — the index, not the store, is what forbids it" {
    const h = fixtures.seededHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);

    const first = try fixtures.mintDirect(conn, fixtures.OWNER_USER_ID, fixtures.MACHINE_NAME);
    defer first.deinit(ALLOC);

    // A raw insert, bypassing `store.mint` entirely — because `mint` revokes
    // before it inserts, and the question here is what happens when something
    // skips that ordering. This is the code path that must be unrepresentable.
    if (conn.exec(INSERT_RAW_CREDENTIAL, .{
        SECOND_ROW_ID,         fixtures.OWNER_USER_ID, fixtures.TENANT_ID,
        fixtures.MACHINE_NAME, REFUSED_HASH,           clock.nowMillis(),
    })) |_| {
        return error.IndexAcceptedASecondLiveCredential;
    } else |_| {}
    try std.testing.expectEqual(
        @as(i64, 1),
        try fixtures.liveCountForMachine(conn, fixtures.OWNER_USER_ID, fixtures.MACHINE_NAME),
    );

    // The index is PARTIAL, and that distinction is load-bearing: once the
    // first row is revoked the same (user, machine) must accept a replacement,
    // or a re-login could never succeed. A plain unique constraint would pass
    // the refusal above and fail here.
    _ = try conn.exec(
        "UPDATE core.cli_credentials SET revoked_at = $2::bigint WHERE id = $1::uuid",
        .{ first.id, clock.nowMillis() },
    );
    _ = try conn.exec(INSERT_RAW_CREDENTIAL, .{
        SECOND_ROW_ID,         fixtures.OWNER_USER_ID, fixtures.TENANT_ID,
        fixtures.MACHINE_NAME, ACCEPTED_HASH,          clock.nowMillis(),
    });
    try std.testing.expectEqual(
        @as(i64, 1),
        try fixtures.liveCountForMachine(conn, fixtures.OWNER_USER_ID, fixtures.MACHINE_NAME),
    );

    // Another machine is a different index entry and was never in contention.
    const other = try fixtures.mintDirect(conn, fixtures.OWNER_USER_ID, fixtures.OTHER_MACHINE_NAME);
    defer other.deinit(ALLOC);
    try std.testing.expectEqual(
        @as(i64, 1),
        try fixtures.liveCountForMachine(conn, fixtures.OWNER_USER_ID, fixtures.OTHER_MACHINE_NAME),
    );

    fixtures.cleanup(h);
}

test "integration: test_mint_records_attribution_and_auth_path_writes_nothing — written once, then never touched" {
    const h = fixtures.seededHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const minted = try fixtures.mint(h, fixtures.TOKEN_OWNER, fixtures.MACHINE_NAME);
    defer minted.deinit();

    const at_mint = try fixtures.wholeRow(h, minted.id);
    defer ALLOC.free(at_mint);

    // Attribution is what makes a shared credential visible without any
    // per-request bookkeeping, so the mint-time facts must actually be there.
    try std.testing.expect(std.mem.indexOf(u8, at_mint, fixtures.MACHINE_NAME) != null);
    try std.testing.expect(std.mem.indexOf(u8, at_mint, "127.0.0.1") != null);
    // Never revoked, and never carrying a usage stamp: the column does not
    // exist, and this is the row that proves nothing quietly added one.
    try std.testing.expect(std.mem.endsWith(u8, at_mint, "|NULL"));

    var i: usize = 0;
    while (i < AUTH_REQUESTS) : (i += 1) {
        const r = try (try h.get(fixtures.PROBE_PATH).bearer(minted.secret)).send();
        defer r.deinit();
        try r.expectStatus(.ok);
    }

    const after = try fixtures.wholeRow(h, minted.id);
    defer ALLOC.free(after);

    // Byte-identical. The sibling tenant-key path stamps `last_used_at` on
    // every authenticated request; this one must stay a pure read, because it
    // is the hottest indexed lookup in the system.
    try std.testing.expectEqualStrings(at_mint, after);

    fixtures.cleanup(h);
}

test "integration: test_failed_mint_leaves_the_prior_credential_live — a failed re-login does not destroy a working terminal" {
    const h = fixtures.seededHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    // The credential this operator is already working with.
    const prior = blk: {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        break :blk try fixtures.mintDirect(conn, fixtures.OWNER_USER_ID, fixtures.MACHINE_NAME);
    };
    defer prior.deinit(ALLOC);

    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try fixtures.blockCredentialInserts(conn);
    }

    {
        // A re-login. Its revoke succeeds and its insert is refused, which is
        // precisely the interleaving that used to leave the operator holding
        // nothing: the revoke had already committed on its own.
        const body = "{\"machine_name\":\"" ++ fixtures.MACHINE_NAME ++ "\"}";
        const r = try (try (try h.post(PATH).bearer(fixtures.TOKEN_OWNER)).json(body)).send();
        defer r.deinit();
        try r.expectErrorCode(ec.ERR_INTERNAL_OPERATION_FAILED);
    }

    {
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        fixtures.unblockCredentialInserts(conn);
        try std.testing.expectEqual(
            @as(i64, 1),
            try fixtures.liveCountForMachine(conn, fixtures.OWNER_USER_ID, fixtures.MACHINE_NAME),
        );
    }

    // And it is the same credential the operator arrived with, still unrevoked
    // — not a replacement that happened to land after the fault was lifted.
    const row = try fixtures.wholeRow(h, prior.id);
    defer ALLOC.free(row);
    try std.testing.expect(std.mem.endsWith(u8, row, "|NULL"));

    fixtures.cleanup(h);
}

/// Stand-ins for the mint-time attribution the probe below does not assert on.
const PROBE_DEPLOYMENT = "http://127.0.0.1:0";
const PROBE_ADDRESS = "127.0.0.1";

// SAFETY: assigned by the test below before `checkAllAllocationFailures` ever
// runs the probe, and the harness owns the connection for the whole test.
var probe_conn: *pg.Conn = undefined;

const MintProbe = struct {
    fn run(alloc: std.mem.Allocator) !void {
        const minted = try store.mint(alloc, probe_conn, .{
            .user_id = fixtures.OWNER_USER_ID,
            .tenant_id = fixtures.TENANT_ID,
            .machine_name = fixtures.MACHINE_NAME,
            .deployment = PROBE_DEPLOYMENT,
            .created_from_address = PROBE_ADDRESS,
        });
        minted.deinit(alloc);
    }
};

test "integration: mint leaks nothing at any allocation-failure point" {
    // `mint` owns two allocations and an `errdefer` for each: the credential
    // itself, then the row identifier. If the second fails, the first must be
    // freed — and nothing but an exhaustive sweep proves that ordering, because
    // the leak only appears on the path a happy-path test never takes.
    //
    // Both allocations sit above BEGIN by design, so an injected failure never
    // strands an open transaction on the pooled connection. That the sweep runs
    // clean against a live datastore is the evidence for it.
    const h = fixtures.seededHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    const conn = try h.acquireConn();
    defer h.releaseConn(conn);
    probe_conn = conn;

    try std.testing.checkAllAllocationFailures(std.testing.allocator, MintProbe.run, .{});

    fixtures.cleanup(h);
}

/// One racer's result. A transport failure is recorded as its own sentinel
/// rather than an error return, so a thread that never reached the server is
/// distinguishable from one the server refused.
const Outcome = struct {
    status: u16 = 0,
    transport_error: ?[]const u8 = null,
};

const TRANSPORT_FAILED: u16 = 599;

const Racer = struct {
    fn run(h: *fixtures.TestHarness, slot: *Outcome) void {
        const body = "{\"machine_name\":\"" ++ fixtures.MACHINE_NAME ++ "\"}";
        const req = h.post(PATH).bearer(fixtures.TOKEN_OWNER) catch |err| return fail(slot, err);
        const with_body = req.json(body) catch |err| return fail(slot, err);
        const r = with_body.send() catch |err| return fail(slot, err);
        defer r.deinit();
        slot.* = .{ .status = r.status };
    }

    fn fail(slot: *Outcome, err: anyerror) void {
        slot.* = .{ .status = TRANSPORT_FAILED, .transport_error = @errorName(err) };
    }
};

test "integration: test_concurrent_logins_leave_one_live_credential — a raced login cannot produce two" {
    const h = fixtures.seededHarness() catch |err| switch (err) {
        error.SkipZigTest => return error.SkipZigTest,
        else => return err,
    };
    defer h.deinit();

    var outcomes = [_]Outcome{.{}} ** CONCURRENT_LOGINS;
    var threads: [CONCURRENT_LOGINS]std.Thread = undefined;
    for (&threads, 0..) |*t, i| {
        t.* = try std.Thread.spawn(.{}, Racer.run, .{ h, &outcomes[i] });
    }
    for (threads) |t| t.join();

    var created: usize = 0;
    for (outcomes) |o| {
        // A racer that never got an answer would make the count below
        // meaningless — it might still be in flight.
        try std.testing.expect(o.transport_error == null);
        if (o.status == @intFromEnum(std.http.Status.created)) created += 1;
    }
    // At least one login succeeded. Asserting EXACTLY one would over-fit to
    // today's no-retry mint: a losing caller that retried against the revoked
    // row would also answer 201, and the invariant below would still hold.
    try std.testing.expect(created >= 1);

    {
        // The invariant the partial unique index exists for. Whatever order the
        // sixteen interleaved in, the datastore can hold only one live row for
        // this (user, machine) — so no operator is left with two credentials
        // they cannot tell apart.
        const conn = try h.acquireConn();
        defer h.releaseConn(conn);
        try std.testing.expectEqual(
            @as(i64, 1),
            try fixtures.liveCountForMachine(conn, fixtures.OWNER_USER_ID, fixtures.MACHINE_NAME),
        );
    }

    fixtures.cleanup(h);
}
