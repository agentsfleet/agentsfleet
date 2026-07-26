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
//! **Shape.** A direct-mapped table of fixed-size rows, no allocator, sized for
//! far more runners than a deployment enrolls. Two token hashes landing on one
//! slot simply evict each other, and an eviction costs exactly one Postgres read
//! — the same thing a cold process pays. Unlike `queue/fleet_group_memo.zig`,
//! which can be lock-free because a wrong answer there costs a redundant Redis
//! command, a wrong answer HERE is an authentication verdict, so the table is
//! mutex-guarded and every read is exact.

const std = @import("std");
const constants = @import("common");

/// Runners tracked per process. Each row is ~112 bytes, so the whole table is
/// ~14 KiB — small enough to be a static global and large enough that a
//  deployment reaches its Postgres ceiling long before it evicts.
pub const MAX_SLOTS: usize = 128;

/// SHA-256 rendered as hex, which is what `fleet.runners.token_hash` stores.
pub const TOKEN_HASH_HEX_LEN: usize = 64;

/// Canonical UUID text — the widest `runner_id` a row can carry.
pub const RUNNER_ID_MAX_LEN: usize = 36;

/// How long a verdict is trusted without re-reading Postgres.
///
/// Pinned to the runner's heartbeat rather than chosen freely: this is the
/// window in which a cordoned, drained, or revoked runner can still authenticate
/// against a machine that did not serve the operator's request, and bounding it
/// by one heartbeat means such a runner is stopped inside the same interval the
/// platform already uses to decide a host is gone.
pub const ENTRY_TTL_MS: i64 = constants.HEARTBEAT_INTERVAL_MS;

const Slot = struct {
    occupied: bool = false,
    token_hash: [TOKEN_HASH_HEX_LEN]u8 = [_]u8{0} ** TOKEN_HASH_HEX_LEN,
    runner_id: [RUNNER_ID_MAX_LEN]u8 = [_]u8{0} ** RUNNER_ID_MAX_LEN,
    runner_id_len: usize = 0,
    active: bool = false,
    expires_at_ms: i64 = 0,
};

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

var g_mutex: constants.Mutex = .{};
var g_slots: [MAX_SLOTS]Slot = [_]Slot{.{}} ** MAX_SLOTS;

fn slotIndex(token_hash_hex: []const u8) usize {
    var h = std.hash.Wyhash.init(0);
    h.update(token_hash_hex);
    return @intCast(h.final() % MAX_SLOTS);
}

/// Compared in constant time to match the posture of the `fleet.runners` lookup
/// this stands in front of. Not load-bearing on its own — an attacker cannot
/// steer the compared bytes without inverting SHA-256 — but a hash comparison in
/// the auth plane should not be the one place that short-circuits.
fn hashMatches(slot: *const Slot, token_hash_hex: []const u8) bool {
    if (token_hash_hex.len != TOKEN_HASH_HEX_LEN) return false;
    return std.crypto.timing_safe.eql(
        [TOKEN_HASH_HEX_LEN]u8,
        slot.token_hash,
        token_hash_hex[0..TOKEN_HASH_HEX_LEN].*,
    );
}

/// The stored verdict for `token_hash_hex`, or null when absent or expired.
///
/// `now_ms` is a parameter rather than a clock read so the expiry boundary is
/// provable without sleeping.
pub fn get(token_hash_hex: []const u8, now_ms: i64) ?Hit {
    if (token_hash_hex.len != TOKEN_HASH_HEX_LEN) return null;
    g_mutex.lock();
    defer g_mutex.unlock();

    const slot = &g_slots[slotIndex(token_hash_hex)];
    if (!slot.occupied) return null;
    if (!hashMatches(slot, token_hash_hex)) return null;
    // Expiry is checked BEFORE the verdict is handed back, and the stale row is
    // dropped rather than left to be re-read: a revoked runner must not keep a
    // slot answering for it.
    if (now_ms >= slot.expires_at_ms) {
        slot.* = .{};
        return null;
    }
    return .{
        .runner_id_buf = slot.runner_id,
        .runner_id_len = slot.runner_id_len,
        .active = slot.active,
    };
}

/// Remember `token_hash_hex`'s verdict until `now_ms + ENTRY_TTL_MS`.
///
/// A `runner_id` wider than a canonical UUID is not stored rather than
/// truncated — a truncated id would authenticate the wrong runner, and skipping
/// the memo only costs the read this exists to avoid.
pub fn put(token_hash_hex: []const u8, runner_id: []const u8, active: bool, now_ms: i64) void {
    if (token_hash_hex.len != TOKEN_HASH_HEX_LEN) return;
    if (runner_id.len == 0 or runner_id.len > RUNNER_ID_MAX_LEN) return;

    g_mutex.lock();
    defer g_mutex.unlock();

    const slot = &g_slots[slotIndex(token_hash_hex)];
    slot.* = .{
        .occupied = true,
        .runner_id_len = runner_id.len,
        .active = active,
        .expires_at_ms = now_ms + ENTRY_TTL_MS,
    };
    @memcpy(slot.token_hash[0..], token_hash_hex[0..TOKEN_HASH_HEX_LEN]);
    @memcpy(slot.runner_id[0..runner_id.len], runner_id);
}

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

    for (&g_slots) |*slot| {
        if (!slot.occupied) continue;
        if (!std.mem.eql(u8, slot.runner_id[0..slot.runner_id_len], runner_id)) continue;
        slot.* = .{};
    }
}

/// Empty the table. Tests only — the process never needs a full flush, because
/// every entry expires on its own and the operator plane drops entries by id.
pub fn resetForTest() void {
    g_mutex.lock();
    defer g_mutex.unlock();
    g_slots = [_]Slot{.{}} ** MAX_SLOTS;
}

test {
    _ = @import("runner_token_cache_test.zig");
}
