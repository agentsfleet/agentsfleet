//! GitHub connect state — a signed, single-use token binding a connect
//! round-trip to the initiating workspace.
//!
//! The callback carries no Bearer (it is a top-level browser redirect from
//! github.com), so this state is the only trust anchor. It is:
//!   * unforgeable — HMAC-SHA256 over the payload with the platform signing
//!     secret, domain-separated by a constant prefix so the key can't be
//!     cross-used against another HMAC surface.
//!   * time-bounded — an embedded expiry the callback rejects past.
//!   * single-use — a Redis nonce DEL'd on first callback (integer reply), so a
//!     captured state cannot be replayed inside its TTL.
//!
//! Wire shape: `base64url(workspace_id "|" nonce "|" exp_ms) "." hex(mac)`.
//! The signature + expiry are pure (`signState`/`verifySignedState`, unit-tested);
//! the nonce single-use is the only I/O (`mint`/`verifyConsume` wrap it).

const std = @import("std");
const common = @import("common");
const queue_redis = @import("../../../../queue/redis.zig");

const HmacSha256 = std.crypto.auth.hmac.sha2.HmacSha256;
const B64 = std.base64.url_safe_no_pad;

const DOMAIN_PREFIX = "ghconnect:v1:";
const NONCE_KEY_PREFIX = "connect:gh:nonce:";
const STATE_TTL_SECONDS: u32 = 600;
const MS_PER_SECOND: i64 = 1000;
const FIELD_SEP: u8 = '|';
const MAC_SEP: u8 = '.';
const NONCE_BYTES = 16;

/// Verified state payload. `workspace_id` and `nonce` borrow from `buf`; the
/// caller owns `buf` and frees it with the same allocator.
pub const Parsed = struct {
    workspace_id: []const u8,
    nonce: []const u8,
    buf: []const u8,
};

/// Build a signed state string. Pure (no I/O) — the unit tests drive this and
/// `verifySignedState` directly. Caller owns the returned slice.
pub fn signState(
    alloc: std.mem.Allocator,
    secret: []const u8,
    workspace_id: []const u8,
    nonce: []const u8,
    exp_ms: i64,
) ![]const u8 {
    const payload = try std.fmt.allocPrint(alloc, "{s}{c}{s}{c}{d}", .{
        workspace_id, FIELD_SEP, nonce, FIELD_SEP, exp_ms,
    });
    defer alloc.free(payload);

    const mac_hex = macHex(secret, payload);
    const b64_buf = try alloc.alloc(u8, B64.Encoder.calcSize(payload.len));
    defer alloc.free(b64_buf);
    const b64 = B64.Encoder.encode(b64_buf, payload);

    return std.fmt.allocPrint(alloc, "{s}{c}{s}", .{ b64, MAC_SEP, mac_hex[0..] });
}

/// Verify a state's signature + expiry (no Redis). Returns the parsed payload
/// (caller frees `.buf`) or null on a bad signature, malformed input, or expiry.
pub fn verifySignedState(
    alloc: std.mem.Allocator,
    secret: []const u8,
    state: []const u8,
    now_ms: i64,
) ?Parsed {
    const dot = std.mem.lastIndexOfScalar(u8, state, MAC_SEP) orelse return null;
    const b64 = state[0..dot];
    const provided_mac = state[dot + 1 ..];

    const decoded_len = B64.Decoder.calcSizeForSlice(b64) catch return null;
    const buf = alloc.alloc(u8, decoded_len) catch return null;
    B64.Decoder.decode(buf, b64) catch return freeNull(alloc, buf);

    const expected = macHex(secret, buf);
    if (!constEql(provided_mac, expected[0..])) return freeNull(alloc, buf);

    var it = std.mem.splitScalar(u8, buf, FIELD_SEP);
    const ws = it.next() orelse return freeNull(alloc, buf);
    const nonce = it.next() orelse return freeNull(alloc, buf);
    const exp_raw = it.next() orelse return freeNull(alloc, buf);
    const exp_ms = std.fmt.parseInt(i64, exp_raw, 10) catch return freeNull(alloc, buf);
    if (now_ms > exp_ms) return freeNull(alloc, buf);

    return .{ .workspace_id = ws, .nonce = nonce, .buf = buf };
}

/// Mint a signed single-use state for `workspace_id`. Records the nonce in Redis
/// (TTL `STATE_TTL_SECONDS`). Caller owns the slice. `now_ms` is injected.
pub fn mint(
    alloc: std.mem.Allocator,
    queue: *queue_redis.Client,
    secret: []const u8,
    workspace_id: []const u8,
    now_ms: i64,
) ![]const u8 {
    var raw: [NONCE_BYTES]u8 = undefined;
    try common.secureRandomBytes(&raw);
    const nonce = std.fmt.bytesToHex(raw, .lower);
    const exp_ms = now_ms + @as(i64, STATE_TTL_SECONDS) * MS_PER_SECOND;

    const state = try signState(alloc, secret, workspace_id, nonce[0..], exp_ms);
    errdefer alloc.free(state);
    try storeNonce(queue, nonce[0..]);
    return state;
}

/// Verify + single-use consume. Returns the bound `workspace_id` (caller owns)
/// or null on any failure (bad signature, malformed, expired, or replayed).
pub fn verifyConsume(
    alloc: std.mem.Allocator,
    queue: *queue_redis.Client,
    secret: []const u8,
    state: []const u8,
    now_ms: i64,
) ?[]const u8 {
    const p = verifySignedState(alloc, secret, state, now_ms) orelse return null;
    defer alloc.free(p.buf);
    if (!consumeNonce(queue, p.nonce)) return null;
    return alloc.dupe(u8, p.workspace_id) catch null;
}

fn freeNull(alloc: std.mem.Allocator, buf: []const u8) ?Parsed {
    alloc.free(buf);
    return null;
}

fn macHex(secret: []const u8, payload: []const u8) [HmacSha256.mac_length * 2]u8 {
    var mac: [HmacSha256.mac_length]u8 = undefined;
    var h = HmacSha256.init(secret);
    h.update(DOMAIN_PREFIX);
    h.update(payload);
    h.final(&mac);
    return std.fmt.bytesToHex(mac, .lower);
}

/// Redis key for a connect nonce — one spelling of the prefix+nonce format so
/// the store and consume sites can't drift (and no duplicated format literal).
fn nonceKey(buf: []u8, nonce: []const u8) ![]const u8 {
    return std.fmt.bufPrint(buf, NONCE_KEY_PREFIX ++ "{s}", .{nonce});
}

fn storeNonce(queue: *queue_redis.Client, nonce: []const u8) !void {
    var key_buf: [128]u8 = undefined;
    try queue.setEx(try nonceKey(&key_buf, nonce), "1", STATE_TTL_SECONDS);
}

/// Atomic single-use: DEL returns the count removed (1 = first use, 0 = already
/// consumed or expired). Mirrors the grant-approval integer reply.
fn consumeNonce(queue: *queue_redis.Client, nonce: []const u8) bool {
    var key_buf: [128]u8 = undefined;
    const key = nonceKey(&key_buf, nonce) catch return false;
    var resp = queue.commandAllowError(&.{ "DEL", key }) catch return false;
    defer resp.deinit(queue.alloc);
    return switch (resp) {
        .integer => |n| n == 1,
        else => false,
    };
}

fn constEql(a: []const u8, b: []const u8) bool {
    if (a.len != b.len) return false;
    var diff: u8 = 0;
    for (a, b) |x, y| diff |= x ^ y;
    return diff == 0;
}

// ── Tests (pure crypto/expiry — the Redis single-use is integration-gated) ───

const testing = std.testing;
const T_SECRET = "test-connect-signing-secret";
const T_WS = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ddd01";
const T_NONCE = "deadbeefdeadbeefdeadbeefdeadbeef";
const T_FAR_FUTURE: i64 = 32_503_680_000_000; // year 3000, ms

test "signState/verifySignedState: round-trips workspace_id + nonce" {
    const st = try signState(testing.allocator, T_SECRET, T_WS, T_NONCE, T_FAR_FUTURE);
    defer testing.allocator.free(st);
    const p = verifySignedState(testing.allocator, T_SECRET, st, 0).?;
    defer testing.allocator.free(p.buf);
    try testing.expectEqualStrings(T_WS, p.workspace_id);
    try testing.expectEqualStrings(T_NONCE, p.nonce);
}

test "verifySignedState: rejects a tampered mac" {
    const st = try signState(testing.allocator, T_SECRET, T_WS, T_NONCE, T_FAR_FUTURE);
    defer testing.allocator.free(st);
    const bad = try testing.allocator.dupe(u8, st);
    defer testing.allocator.free(bad);
    bad[bad.len - 1] = if (bad[bad.len - 1] == 'a') 'b' else 'a'; // flip a mac hex digit
    try testing.expect(verifySignedState(testing.allocator, T_SECRET, bad, 0) == null);
}

test "verifySignedState: rejects a foreign secret" {
    const st = try signState(testing.allocator, T_SECRET, T_WS, T_NONCE, T_FAR_FUTURE);
    defer testing.allocator.free(st);
    try testing.expect(verifySignedState(testing.allocator, "other-secret", st, 0) == null);
}

test "verifySignedState: rejects an expired state" {
    const st = try signState(testing.allocator, T_SECRET, T_WS, T_NONCE, 1000);
    defer testing.allocator.free(st);
    try testing.expect(verifySignedState(testing.allocator, T_SECRET, st, 2000) == null);
}

test "verifySignedState: rejects malformed input (no separator)" {
    try testing.expect(verifySignedState(testing.allocator, T_SECRET, "not-a-state", 0) == null);
}
