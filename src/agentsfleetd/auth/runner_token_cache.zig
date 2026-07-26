//! Per-process memo of runner token verdicts, so an idle runner's polling does
//! not cost one Postgres read per request.
//!
//! A runner's `agt_r` is an OPAQUE secret: its only meaning is "a row exists
//! whose `token_hash` is this", so it cannot be checked without going to look.
//! That is one indexed `fleet.runners` read on every heartbeat, lease poll,
//! report, and activity call — a cost that scales with runner count and with
//! poll rate, and that a runner sitting idle pays forever. (The human plane has
//! no equivalent cost: a Clerk JWT carries its own proof and verifies against a
//! cached public key.) This memo removes it for the steady state.
//!
//! **The entry expires, and that expiry is the correctness bound.** The machine
//! that serves an operator's cordon / drain / revoke drops the entry itself, so
//! on a single-machine deployment a state change bites on the very next request.
//! Once the control plane runs more than one machine the operator's request
//! lands on ONE of them, and the others keep answering from memory until their
//! entry expires. `ENTRY_TTL_MS` is therefore the worst-case window in which a
//! runner taken out of service is still authenticated, and it is pinned to the
//! runner's own heartbeat interval so that window can never exceed one liveness
//! cycle.
//!
//! **What is stored.** The SHA-256 hex of the token — never the token. That is
//! the same value `fleet.runners.token_hash` holds, so this memo widens no
//! secret's blast radius: an attacker who can read this process's memory can
//! already read the pool credentials next to it.
//!
//! **Shape.** `common.CacheTable` — allocator-free, fixed capacity, entries
//! expiring on their own. Every read here is an authentication verdict, so the
//! table is under an exclusive mutex and every hit is an exact match; the
//! lock-free tolerance `queue/fleet_group_memo.zig` enjoys does not transfer.

const std = @import("std");
const constants = @import("common");

/// SHA-256 rendered as hex, which is what `fleet.runners.token_hash` stores.
pub const TOKEN_HASH_HEX_LEN: usize = 64;

/// Canonical UUID text — the widest `runner_id` a row can carry.
pub const RUNNER_ID_MAX_LEN: usize = 36;

/// Buckets, each holding `ENTRIES_PER_BUCKET` tokens. Set-associative rather
/// than direct-mapped: colliding tokens coexist instead of evicting each other
/// on every interleaved request, which is what made a direct-mapped table worst
/// at the scale it exists for. At the ~100 runners
/// `schema/033_hot_path_indexes.sql` assumes, 1024 buckets leave collisions rare
/// and 4-deep buckets absorb the ones that happen, so a live token is
/// effectively never evicted by another live token.
const BUCKET_COUNT: usize = 1024;
const ENTRIES_PER_BUCKET: u8 = 4;

/// Leading digest bytes used as the bucket index.
const BUCKET_KEY_BYTES: usize = 4;

/// How long a verdict is trusted without re-reading Postgres.
///
/// Pinned to the runner's heartbeat rather than chosen freely: this is the
/// window in which a cordoned, drained, or revoked runner can still authenticate
/// against a machine that did not serve the operator's request, and bounding it
/// by one heartbeat means such a runner is stopped inside the same interval the
/// platform already uses to decide a host is gone.
pub const ENTRY_TTL_MS: i64 = constants.HEARTBEAT_INTERVAL_MS;

const TokenHash = [TOKEN_HASH_HEX_LEN]u8;

/// A verdict copied OUT of the table. Returned by value rather than as a slice
/// into a slot, so the caller can never read a row a later `put` has recycled.
pub const Hit = struct {
    runner_id_buf: [RUNNER_ID_MAX_LEN]u8,
    runner_id_len: usize,
    active: bool,

    pub fn runnerId(self: *const Hit) []const u8 {
        return self.runner_id_buf[0..self.runner_id_len];
    }
};

const TokenContext = struct {
    /// SHA-256 output is uniformly distributed, so its leading bytes are already
    /// a perfect bucket index and re-hashing the key would cost more than it
    /// buys (the observation is Bun's `SSLContextCache`). The hex is DECODED
    /// rather than read as raw bytes because ASCII hex characters are not
    /// uniform in their low bits — which is exactly where the bucket mask looks.
    pub fn hash(_: *const TokenContext, key: TokenHash) u64 {
        var raw: [BUCKET_KEY_BYTES]u8 = undefined;
        _ = std.fmt.hexToBytes(&raw, key[0 .. BUCKET_KEY_BYTES * 2]) catch return 0;
        return std.mem.readInt(u32, &raw, .little);
    }

    /// Compared in constant time to match the posture of the `fleet.runners`
    /// lookup this stands in front of. Not load-bearing on its own — an attacker
    /// cannot steer the compared bytes without inverting SHA-256 — but a hash
    /// comparison in the auth plane should not be the one place that
    /// short-circuits.
    pub fn eql(_: *const TokenContext, a: TokenHash, b: TokenHash) bool {
        return std.crypto.timing_safe.eql(TokenHash, a, b);
    }
};

const TokenTable = constants.CacheTable(TokenHash, Hit, TokenContext, .{
    .bucket_count = BUCKET_COUNT,
    .bucket_size = ENTRIES_PER_BUCKET,
});

var g_mutex: constants.Mutex = .{};
var g_table: TokenTable = TokenTable.init(.{});
/// Bumped by every invalidation. A caller reads it BEFORE its Postgres lookup
/// and hands it back to `put`, which refuses to store a verdict the operator
/// plane already invalidated mid-flight. Without it the lookup path loses that
/// race in the one direction that matters: a revoke lands and clears an empty
/// table, then the in-flight read stores the pre-revoke `active` verdict and the
/// revoked runner keeps authenticating for a full expiry window — on the very
/// machine that served the revoke.
var g_generation: u64 = 0;

/// The invalidation counter as of now. Read before a lookup, passed to `put`.
pub fn generation() u64 {
    g_mutex.lock();
    defer g_mutex.unlock();
    return g_generation;
}

fn asKey(token_hash_hex: []const u8) ?TokenHash {
    if (token_hash_hex.len != TOKEN_HASH_HEX_LEN) return null;
    return token_hash_hex[0..TOKEN_HASH_HEX_LEN].*;
}

/// The stored verdict for `token_hash_hex`, or null when absent or expired.
///
/// `now_ms` is a parameter rather than a clock read so the expiry boundary is
/// provable without sleeping.
pub fn get(token_hash_hex: []const u8, now_ms: i64) ?Hit {
    const key = asKey(token_hash_hex) orelse return null;
    g_mutex.lock();
    defer g_mutex.unlock();
    // The mutating reader, so an expired row is dropped rather than left to be
    // re-read: a revoked runner must not keep a slot answering for it.
    return g_table.get(key, now_ms);
}

/// Remember `token_hash_hex`'s verdict until `now_ms + ENTRY_TTL_MS`.
///
/// A `runner_id` wider than a canonical UUID is not stored rather than
/// truncated — a truncated id would authenticate the wrong runner, and skipping
/// the memo only costs the read this exists to avoid.
///
/// `seen_generation` is the value `generation()` returned before the caller read
/// Postgres. If an invalidation landed since, the row the caller is holding may
/// predate it, so the write is dropped and the next request re-reads.
pub fn put(token_hash_hex: []const u8, runner_id: []const u8, active: bool, now_ms: i64, seen_generation: u64) void {
    const key = asKey(token_hash_hex) orelse return;
    if (runner_id.len == 0 or runner_id.len > RUNNER_ID_MAX_LEN) return;

    var hit: Hit = .{
        .runner_id_buf = @splat(0),
        .runner_id_len = runner_id.len,
        .active = active,
    };
    @memcpy(hit.runner_id_buf[0..runner_id.len], runner_id);

    g_mutex.lock();
    defer g_mutex.unlock();
    if (g_generation != seen_generation) return;
    _ = g_table.put(key, hit, now_ms + ENTRY_TTL_MS, now_ms);
}

const RunnerMatch = struct {
    runner_id: []const u8,

    pub fn match(self: RunnerMatch, _: TokenHash, hit: Hit) bool {
        return std.mem.eql(u8, hit.runner_id_buf[0..hit.runner_id_len], self.runner_id);
    }
};

/// Drop every entry for `runner_id`. Called by the operator-plane writes that
/// change what a verdict should say — the admin-state transition and the record
/// delete — after their Postgres write commits.
///
/// Keyed by runner id and not by token hash because those call sites hold the
/// id and must never hold the token. That makes this a scan of the table rather
/// than a slot lookup, which is the right trade: it runs on operator actions,
/// never on the request path.
pub fn invalidateRunner(runner_id: []const u8) void {
    if (runner_id.len == 0 or runner_id.len > RUNNER_ID_MAX_LEN) return;
    g_mutex.lock();
    defer g_mutex.unlock();
    // Bump FIRST and unconditionally — including when the scan below finds
    // nothing. An empty table is exactly the state a racing lookup is about to
    // fill, and that write is the one this counter has to refuse.
    g_generation +%= 1;
    _ = g_table.removeMatching(RunnerMatch{ .runner_id = runner_id });
}

/// Empty the table. Tests only — the process never needs a full flush, because
/// every entry expires on its own and the operator plane drops entries by id.
pub fn resetForTest() void {
    g_mutex.lock();
    defer g_mutex.unlock();
    g_table.clear();
    g_generation = 0;
}

test {
    _ = @import("runner_token_cache_test.zig");
}
