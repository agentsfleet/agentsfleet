//! Redis-backed integration tests for SessionStore + Lua-EVAL atomicity.
//! Pins live behaviour of the verify-and-consume + pod-survival paths
//! that session_store_redis_proto.zig's pure-tests can only smoke-test.
//! Each test owns a fresh session_id (UUIDv7 minted by SessionStore.create)
//! and DELs its own key on teardown — no FLUSHDB, since sibling suites
//! share the namespace. Self-skips when TEST_REDIS_TLS_URL is unset; CI
//! runs against the production-target Upstash flavour so the atomicity
//! claim survives the script-time cap + cjson edge cases vanilla Redis 7
//! does not enforce.

const std = @import("std");
const queue_redis = @import("../queue/redis.zig");
const session_store_redis = @import("session_store_redis.zig");
const session_state = @import("session_state.zig");

const SessionStore = session_store_redis.SessionStore;
const VerifyOutcome = session_store_redis.VerifyOutcome;
const SessionStatus = session_state.SessionStatus;

const TEST_REDIS_URL_ENV: []const u8 = "TEST_REDIS_TLS_URL";

// Stable across the file so HMAC outputs are deterministic test-to-test.
// Real prod peppers come from boot-time env loaders; the values here are
// arbitrary 32-ish-byte strings.
const TEST_CODE_PEPPER: []const u8 = "test-pepper-bytes-32-len--padded";
const TEST_AUDIT_PEPPER: []const u8 = "test-audit-pepper-bytes-32--pad_";

const TEST_CLI_PK: []const u8 = "BASE64URL_SPKI_CLI_PUBLIC_KEY_PLACEHOLDER";
const TEST_DASH_PK: []const u8 = "BASE64URL_SPKI_DASH_PUBLIC_KEY_PLACEHOLDER";
const TEST_CIPHERTEXT: []const u8 = "BASE64URL_AEAD_CIPHERTEXT_PLACEHOLDER";
const TEST_NONCE: []const u8 = "BASE64URL_AEAD_NONCE_PLACEHOLDER";
const TEST_TOKEN_NAME: []const u8 = "integration-test";
const TEST_CLERK_USER_ID: []const u8 = "user_integration_test_42";
const TEST_VERIFICATION_CODE: []const u8 = "123456";

// 64-char hex fingerprints — Lua compares byte-for-byte so any
// fixed-length hex works. Two distinct fingerprints exercise the
// different-source replay path.
const FP_A: []const u8 = "a" ** 64;
const FP_B: []const u8 = "b" ** 64;

fn connectRedisOrSkip(alloc: std.mem.Allocator) !queue_redis.Client {
    const url = std.process.getEnvVarOwned(alloc, TEST_REDIS_URL_ENV) catch return error.SkipZigTest;
    defer alloc.free(url);
    return queue_redis.Client.connectFromUrl(alloc, url);
}

fn delSessionKey(client: *queue_redis.Client, alloc: std.mem.Allocator, session_id: []const u8) void {
    var key_buf: [session_store_redis.SESSION_KEY_PREFIX.len + 36]u8 = undefined;
    const key = std.fmt.bufPrint(
        &key_buf,
        "{s}{s}",
        .{ session_store_redis.SESSION_KEY_PREFIX, session_id },
    ) catch return;
    var resp = client.command(&.{ "DEL", key }) catch return;
    resp.deinit(alloc);
}

/// Create + approve a session so verify tests start from
/// `verification_pending`. Caller owns the returned session_id and is
/// expected to register a `delSessionKey` defer for cleanup.
fn createApprovedSession(store: *SessionStore) ![]const u8 {
    const sid = try store.create(TEST_CLI_PK, TEST_TOKEN_NAME);
    errdefer store.alloc.free(sid);
    try store.approve(
        sid,
        TEST_DASH_PK,
        TEST_CIPHERTEXT,
        TEST_NONCE,
        TEST_VERIFICATION_CODE,
        TEST_CLERK_USER_ID,
    );
    return sid;
}

/// Mutate the stored blob to push the consume-idempotency window
/// timestamp into the past — surrogate for the 61-second wall-clock
/// wait in test_verify_replay_rejected_outside_window. Touches the
/// blob through SET, preserving every other field.
fn expireReplayWindow(
    store: *SessionStore,
    session_id: []const u8,
) !void {
    var parsed = (try store.get(session_id)) orelse return error.SessionGone;
    defer parsed.deinit();
    var mutated = parsed.value;
    mutated.consume_payload_expires_at_ms = std.time.milliTimestamp() - 1;
    const blob = try session_state.encode(store.alloc, mutated);
    defer store.alloc.free(blob);
    var key_buf: [session_store_redis.SESSION_KEY_PREFIX.len + 36]u8 = undefined;
    const key = try std.fmt.bufPrint(
        &key_buf,
        "{s}{s}",
        .{ session_store_redis.SESSION_KEY_PREFIX, session_id },
    );
    try store.client.setEx(key, blob, session_store_redis.SESSION_TTL_SECONDS);
}

// ── Tests ──────────────────────────────────────────────────────────────

test "delete is idempotent: first call aborts, second reports already_aborted" {
    const alloc = std.testing.allocator;
    var client = try connectRedisOrSkip(alloc);
    defer client.deinit();
    var store = SessionStore.init(alloc, &client, TEST_CODE_PEPPER, TEST_AUDIT_PEPPER);

    const sid = try createApprovedSession(&store);
    defer alloc.free(sid);
    defer delSessionKey(&client, alloc, sid);

    // First DELETE performs the abort.
    try std.testing.expectEqual(
        session_store_redis.DeleteOutcome.aborted,
        try store.delete(sid, TEST_CLERK_USER_ID),
    );
    // Second DELETE is idempotent and must report `.already_aborted` — the
    // handler keys its audit emit on `.aborted`, so a re-delete writes no
    // duplicate explicit_cancel record over the prior reason.
    try std.testing.expectEqual(
        session_store_redis.DeleteOutcome.already_aborted,
        try store.delete(sid, TEST_CLERK_USER_ID),
    );
}

test "verify is atomic: consume blocks replay from a different fingerprint" {
    const alloc = std.testing.allocator;
    var client = try connectRedisOrSkip(alloc);
    defer client.deinit();
    var store = SessionStore.init(alloc, &client, TEST_CODE_PEPPER, TEST_AUDIT_PEPPER);

    const sid = try createApprovedSession(&store);
    defer alloc.free(sid);
    defer delSessionKey(&client, alloc, sid);

    var first = try store.verifyAndConsume(sid, TEST_VERIFICATION_CODE, FP_A);
    defer first.deinit(alloc);
    try std.testing.expect(first == .success);
    try std.testing.expectEqualStrings(TEST_CIPHERTEXT, first.success.ciphertext);
    try std.testing.expectEqualStrings(TEST_DASH_PK, first.success.dashboard_public_key);
    try std.testing.expectEqualStrings(TEST_NONCE, first.success.nonce);

    var second = try store.verifyAndConsume(sid, TEST_VERIFICATION_CODE, FP_B);
    defer second.deinit(alloc);
    try std.testing.expect(second == .consumed);

    var parsed = (try store.get(sid)).?;
    defer parsed.deinit();
    try std.testing.expectEqual(SessionStatus.consumed, parsed.value.status);
    try std.testing.expect(parsed.value.consumed_at_ms != null);
    try std.testing.expect(parsed.value.consume_payload_expires_at_ms != null);
}

test "verify locks out after MAX_VERIFY_ATTEMPTS wrong codes → terminal rate_limited" {
    const alloc = std.testing.allocator;
    var client = try connectRedisOrSkip(alloc);
    defer client.deinit();
    var store = SessionStore.init(alloc, &client, TEST_CODE_PEPPER, TEST_AUDIT_PEPPER);

    const sid = try createApprovedSession(&store);
    defer alloc.free(sid);
    defer delSessionKey(&client, alloc, sid);

    const wrong_code: []const u8 = "000000";

    // The first MAX-1 wrong attempts increment the counter and stay retryable.
    var attempt: u8 = 1;
    while (attempt < session_store_redis.MAX_VERIFY_ATTEMPTS) : (attempt += 1) {
        var out = try store.verifyAndConsume(sid, wrong_code, FP_A);
        defer out.deinit(alloc);
        try std.testing.expect(out == .invalid_code);
        try std.testing.expectEqual(attempt, out.invalid_code);
    }

    // The attempt that trips the cap returns the terminal rate_limited tag —
    // distinct from invalid_code so the handler emits the abort audit once.
    var locked = try store.verifyAndConsume(sid, wrong_code, FP_A);
    defer locked.deinit(alloc);
    try std.testing.expect(locked == .rate_limited);

    // The session is now aborted; a re-verify (even the correct code) returns
    // the already-terminal aborted outcome, never rate_limited a second time.
    var after = try store.verifyAndConsume(sid, TEST_VERIFICATION_CODE, FP_A);
    defer after.deinit(alloc);
    try std.testing.expect(after == .aborted);
    try std.testing.expectEqualStrings("rate_limit_exceeded", after.aborted); // pin test: literal is the contract

    var parsed = (try store.get(sid)).?;
    defer parsed.deinit();
    try std.testing.expectEqual(SessionStatus.aborted, parsed.value.status);
}

test "verify is idempotent within the replay window for the same fingerprint" {
    const alloc = std.testing.allocator;
    var client = try connectRedisOrSkip(alloc);
    defer client.deinit();
    var store = SessionStore.init(alloc, &client, TEST_CODE_PEPPER, TEST_AUDIT_PEPPER);

    const sid = try createApprovedSession(&store);
    defer alloc.free(sid);
    defer delSessionKey(&client, alloc, sid);

    var first = try store.verifyAndConsume(sid, TEST_VERIFICATION_CODE, FP_A);
    defer first.deinit(alloc);
    try std.testing.expect(first == .success);

    var second = try store.verifyAndConsume(sid, TEST_VERIFICATION_CODE, FP_A);
    defer second.deinit(alloc);
    try std.testing.expect(second == .replay);
    try std.testing.expectEqualStrings(first.success.ciphertext, second.replay.ciphertext);
    try std.testing.expectEqualStrings(first.success.nonce, second.replay.nonce);
    try std.testing.expectEqualStrings(
        first.success.dashboard_public_key,
        second.replay.dashboard_public_key,
    );

    const before_consumed_at = blk: {
        var parsed = (try store.get(sid)).?;
        defer parsed.deinit();
        break :blk parsed.value.consumed_at_ms.?;
    };

    // Replay must not bump consumed_at_ms — pin Fix 1.
    var third = try store.verifyAndConsume(sid, TEST_VERIFICATION_CODE, FP_A);
    defer third.deinit(alloc);
    try std.testing.expect(third == .replay);
    var reparsed = (try store.get(sid)).?;
    defer reparsed.deinit();
    try std.testing.expectEqual(before_consumed_at, reparsed.value.consumed_at_ms.?);
}

test "verify rejects replay once the 60s payload window has elapsed" {
    const alloc = std.testing.allocator;
    var client = try connectRedisOrSkip(alloc);
    defer client.deinit();
    var store = SessionStore.init(alloc, &client, TEST_CODE_PEPPER, TEST_AUDIT_PEPPER);

    const sid = try createApprovedSession(&store);
    defer alloc.free(sid);
    defer delSessionKey(&client, alloc, sid);

    var first = try store.verifyAndConsume(sid, TEST_VERIFICATION_CODE, FP_A);
    defer first.deinit(alloc);
    try std.testing.expect(first == .success);

    // Surrogate for the 61-second wait — push the window into the past
    // so the Lua's `window_open` check fails.
    try expireReplayWindow(&store, sid);

    var after = try store.verifyAndConsume(sid, TEST_VERIFICATION_CODE, FP_A);
    defer after.deinit(alloc);
    try std.testing.expect(after == .consumed);
}

test "verify rejects replay from a different fingerprint inside the window" {
    const alloc = std.testing.allocator;
    var client = try connectRedisOrSkip(alloc);
    defer client.deinit();
    var store = SessionStore.init(alloc, &client, TEST_CODE_PEPPER, TEST_AUDIT_PEPPER);

    const sid = try createApprovedSession(&store);
    defer alloc.free(sid);
    defer delSessionKey(&client, alloc, sid);

    var first = try store.verifyAndConsume(sid, TEST_VERIFICATION_CODE, FP_A);
    defer first.deinit(alloc);
    try std.testing.expect(first == .success);

    // Inside the 60s window but the fingerprint differs — Lua falls
    // through the replay-cache branch and returns the consumed terminal.
    var cross_source = try store.verifyAndConsume(sid, TEST_VERIFICATION_CODE, FP_B);
    defer cross_source.deinit(alloc);
    try std.testing.expect(cross_source == .consumed);
}

test "verify under two concurrent correct-code calls collapses to one consume" {
    const alloc = std.testing.allocator;
    var client = try connectRedisOrSkip(alloc);
    defer client.deinit();
    var store = SessionStore.init(alloc, &client, TEST_CODE_PEPPER, TEST_AUDIT_PEPPER);

    const sid = try createApprovedSession(&store);
    defer alloc.free(sid);
    defer delSessionKey(&client, alloc, sid);

    const Racer = struct {
        store: *SessionStore,
        sid: []const u8,
        outcome: ?VerifyOutcome = null,
        err: ?anyerror = null,

        fn run(self: *@This()) void {
            self.outcome = self.store.verifyAndConsume(
                self.sid,
                TEST_VERIFICATION_CODE,
                FP_A,
            ) catch |e| {
                self.err = e;
                return;
            };
        }
    };

    // Two SessionStore.verifyAndConsume calls hitting the same key
    // simultaneously. Lua-EVAL is atomic per single-threaded Redis, so
    // one wins as .success and the other re-enters on a consumed blob;
    // same fingerprint + still within the 60s window → .replay.
    //
    // NOTE: This test races two clients through a single shared Client
    // struct. queue_redis.Client serialises commands internally so the
    // racer behaviour is "two callers funnel through one connection,
    // Lua-EVAL atomicity is what guarantees consume-once." For the
    // separate-connection racer variant (true parallelism on the
    // Redis side) see test_redis_sessionstore_concurrent_pods.
    var racer_a = Racer{ .store = &store, .sid = sid };
    var racer_b = Racer{ .store = &store, .sid = sid };
    const ta = try std.Thread.spawn(.{}, Racer.run, .{&racer_a});
    const tb = try std.Thread.spawn(.{}, Racer.run, .{&racer_b});
    ta.join();
    tb.join();

    try std.testing.expect(racer_a.err == null);
    try std.testing.expect(racer_b.err == null);
    try std.testing.expect(racer_a.outcome != null);
    try std.testing.expect(racer_b.outcome != null);

    // verifyAndConsume returns owned outcomes (post-UAF-fix); deinit
    // both before scope-exit to keep the test allocator leak-free.
    defer racer_a.outcome.?.deinit(alloc);
    defer racer_b.outcome.?.deinit(alloc);

    var success_count: u8 = 0;
    var replay_count: u8 = 0;
    for ([_]VerifyOutcome{ racer_a.outcome.?, racer_b.outcome.? }) |out| {
        switch (out) {
            .success => success_count += 1,
            .replay => replay_count += 1,
            else => return error.UnexpectedRaceOutcome,
        }
    }
    try std.testing.expectEqual(@as(u8, 1), success_count);
    try std.testing.expectEqual(@as(u8, 1), replay_count);

    var parsed = (try store.get(sid)).?;
    defer parsed.deinit();
    try std.testing.expectEqual(SessionStatus.consumed, parsed.value.status);
    try std.testing.expect(parsed.value.consumed_at_ms != null);
}

test "session_store survives pod restart" {
    const alloc = std.testing.allocator;

    // Pod A: connect, write, close. The session_id is heap-allocated via
    // the same alloc the test owns, so it outlives client_a.deinit().
    const sid = blk: {
        var client_a = try connectRedisOrSkip(alloc);
        defer client_a.deinit();
        var store_a = SessionStore.init(alloc, &client_a, TEST_CODE_PEPPER, TEST_AUDIT_PEPPER);
        break :blk try createApprovedSession(&store_a);
    };
    defer alloc.free(sid);

    // Pod B: fresh connection, same Redis target, same peppers. Reading
    // back the blob proves the data outlives pod A; running a full
    // verifyAndConsume proves the HMAC pepper round-trips correctly
    // across pod boundaries (Invariant 14).
    var client_b = try connectRedisOrSkip(alloc);
    defer client_b.deinit();
    var store_b = SessionStore.init(alloc, &client_b, TEST_CODE_PEPPER, TEST_AUDIT_PEPPER);
    defer delSessionKey(&client_b, alloc, sid);

    var parsed = (try store_b.get(sid)).?;
    defer parsed.deinit();
    try std.testing.expectEqualStrings(sid, parsed.value.session_id);
    try std.testing.expectEqual(SessionStatus.verification_pending, parsed.value.status);
    try std.testing.expectEqualStrings(TEST_CLERK_USER_ID, parsed.value.clerk_user_id.?);
    try std.testing.expectEqualStrings(TEST_DASH_PK, parsed.value.dashboard_public_key.?);

    var out = try store_b.verifyAndConsume(sid, TEST_VERIFICATION_CODE, FP_A);
    defer out.deinit(alloc);
    try std.testing.expect(out == .success);
    try std.testing.expectEqualStrings(TEST_CIPHERTEXT, out.success.ciphertext);
}

test "session_store works across three concurrent pods" {
    const alloc = std.testing.allocator;

    var client_a = try connectRedisOrSkip(alloc);
    defer client_a.deinit();
    var client_b = try connectRedisOrSkip(alloc);
    defer client_b.deinit();
    var client_c = try connectRedisOrSkip(alloc);
    defer client_c.deinit();

    var store_a = SessionStore.init(alloc, &client_a, TEST_CODE_PEPPER, TEST_AUDIT_PEPPER);
    var store_b = SessionStore.init(alloc, &client_b, TEST_CODE_PEPPER, TEST_AUDIT_PEPPER);
    var store_c = SessionStore.init(alloc, &client_c, TEST_CODE_PEPPER, TEST_AUDIT_PEPPER);

    // Pod A: create + approve.
    const sid = try createApprovedSession(&store_a);
    defer alloc.free(sid);
    defer delSessionKey(&client_c, alloc, sid);

    // Pod B: poll. The dashboard would do this on the `/cli-auth/{id}`
    // page to render the verification code prompt.
    {
        var parsed = (try store_b.get(sid)).?;
        defer parsed.deinit();
        try std.testing.expectEqual(SessionStatus.verification_pending, parsed.value.status);
        try std.testing.expectEqualStrings(TEST_CLERK_USER_ID, parsed.value.clerk_user_id.?);
    }

    // Pod C: verify. Different connection from approve+poll, but the
    // Lua-EVAL atomicity is across the Redis instance not the client,
    // so .success is the expected outcome.
    var out = try store_c.verifyAndConsume(sid, TEST_VERIFICATION_CODE, FP_A);
    defer out.deinit(alloc);
    try std.testing.expect(out == .success);
    try std.testing.expectEqualStrings(TEST_CIPHERTEXT, out.success.ciphertext);

    // Polling from pod B again should now see the terminal consumed
    // state — the EVAL on pod C wrote-through to the same key.
    var after = (try store_b.get(sid)).?;
    defer after.deinit();
    try std.testing.expectEqual(SessionStatus.consumed, after.value.status);
}
