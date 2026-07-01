//! Shared connector OAuth install-state — a signed, single-use token binding a
//! connect round-trip to the initiating workspace, reused by every OAuth
//! connector (GitHub, Slack, …). A connector callback carries no Bearer (it is a
//! top-level browser redirect), so this state is the only trust anchor:
//!   * unforgeable — HMAC-SHA256 over the payload with the platform signing
//!     secret, domain-separated by a per-connector prefix (`Config.domain_prefix`)
//!     so one connector's state can't cross-verify as another's.
//!   * time-bounded — an embedded expiry the callback rejects past.
//!   * single-use — a Redis nonce DEL'd on first callback (integer reply), keyed
//!     by a per-connector prefix (`Config.nonce_prefix`).
//!
//! Wire shape: `base64url(workspace_id "|" nonce "|" exp_ms) "." hex(mac)`.
//! Each connector pins one `Config` in a thin `connectors/<name>/state.zig`
//! wrapper and calls `mint`/`verifyConsume`; `signState`/`verifySignedState` are
//! the pure (I/O-free) core the unit tests drive.

const std = @import("std");
const common = @import("common");
const queue_redis = @import("../../../queue/redis.zig");

const HmacSha256 = std.crypto.auth.hmac.sha2.HmacSha256;
const B64 = std.base64.url_safe_no_pad;

const MS_PER_SECOND: i64 = 1000;
const FIELD_SEP: u8 = '|';
const MAC_SEP: u8 = '.';
const NONCE_BYTES = 16;
const NONCE_KEY_BUF_LEN = 128;
const DEFAULT_TTL_SECONDS: u32 = 600;

/// Per-connector binding: the HMAC domain prefix and Redis nonce-key prefix that
/// keep one connector's states from cross-verifying as another's, plus the state
/// lifetime. Pinned once per connector in its `connectors/<name>/state.zig`.
pub const Config = struct {
    domain_prefix: []const u8,
    nonce_prefix: []const u8,
    ttl_seconds: u32 = DEFAULT_TTL_SECONDS,
};

/// Verified state payload. `workspace_id` and `nonce` borrow from `buf`; the
/// caller owns `buf` and frees it with the same allocator.
const Parsed = struct {
    workspace_id: []const u8,
    nonce: []const u8,
    buf: []const u8,
};

/// Mint a signed single-use state for `workspace_id`. Records the nonce in Redis
/// (TTL `cfg.ttl_seconds`). Caller owns the returned slice. `now_ms` is injected.
pub fn mint(
    alloc: std.mem.Allocator,
    queue: *queue_redis.Client,
    cfg: Config,
    secret: []const u8,
    workspace_id: []const u8,
    now_ms: i64,
) ![]const u8 {
    var raw: [NONCE_BYTES]u8 = undefined;
    try common.secureRandomBytes(&raw);
    const nonce = std.fmt.bytesToHex(raw, .lower);
    const exp_ms = now_ms + @as(i64, cfg.ttl_seconds) * MS_PER_SECOND;

    const state = try signState(alloc, cfg, secret, workspace_id, nonce[0..], exp_ms);
    errdefer alloc.free(state);
    try storeNonce(queue, cfg, nonce[0..]);
    return state;
}

/// Verify + single-use consume. Returns the bound `workspace_id` (caller owns)
/// or null on any failure (bad signature, malformed, expired, or replayed).
pub fn verifyConsume(
    alloc: std.mem.Allocator,
    queue: *queue_redis.Client,
    cfg: Config,
    secret: []const u8,
    state: []const u8,
    now_ms: i64,
) ?[]const u8 {
    const p = verifySignedState(alloc, cfg, secret, state, now_ms) orelse return null;
    defer alloc.free(p.buf);
    if (!consumeNonce(queue, cfg, p.nonce)) return null;
    return alloc.dupe(u8, p.workspace_id) catch null;
}

/// Build a signed state string. Pure (no I/O). Caller owns the returned slice.
fn signState(
    alloc: std.mem.Allocator,
    cfg: Config,
    secret: []const u8,
    workspace_id: []const u8,
    nonce: []const u8,
    exp_ms: i64,
) ![]const u8 {
    const payload = try std.fmt.allocPrint(alloc, "{s}{c}{s}{c}{d}", .{
        workspace_id, FIELD_SEP, nonce, FIELD_SEP, exp_ms,
    });
    defer alloc.free(payload);

    const mac_hex = macHex(cfg.domain_prefix, secret, payload);
    const b64_buf = try alloc.alloc(u8, B64.Encoder.calcSize(payload.len));
    defer alloc.free(b64_buf);
    const b64 = B64.Encoder.encode(b64_buf, payload);

    return std.fmt.allocPrint(alloc, "{s}{c}{s}", .{ b64, MAC_SEP, mac_hex[0..] });
}

/// Verify a state's signature + expiry (no Redis). Returns the parsed payload
/// (caller frees `.buf`) or null on a bad signature, malformed input, or expiry.
fn verifySignedState(
    alloc: std.mem.Allocator,
    cfg: Config,
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

    const expected = macHex(cfg.domain_prefix, secret, buf);
    if (!constEql(provided_mac, expected[0..])) return freeNull(alloc, buf);

    var it = std.mem.splitScalar(u8, buf, FIELD_SEP);
    const ws = it.next() orelse return freeNull(alloc, buf);
    const nonce = it.next() orelse return freeNull(alloc, buf);
    const exp_raw = it.next() orelse return freeNull(alloc, buf);
    const exp_ms = std.fmt.parseInt(i64, exp_raw, 10) catch return freeNull(alloc, buf);
    if (now_ms > exp_ms) return freeNull(alloc, buf);

    return .{ .workspace_id = ws, .nonce = nonce, .buf = buf };
}

fn freeNull(alloc: std.mem.Allocator, buf: []const u8) ?Parsed {
    alloc.free(buf);
    return null;
}

fn macHex(domain_prefix: []const u8, secret: []const u8, payload: []const u8) [HmacSha256.mac_length * 2]u8 {
    var mac: [HmacSha256.mac_length]u8 = undefined;
    var h = HmacSha256.init(secret);
    h.update(domain_prefix);
    h.update(payload);
    h.final(&mac);
    return std.fmt.bytesToHex(mac, .lower);
}

/// Redis key for a connect nonce — one spelling of prefix+nonce so the store and
/// consume sites can't drift.
fn nonceKey(buf: []u8, nonce_prefix: []const u8, nonce: []const u8) ![]const u8 {
    return std.fmt.bufPrint(buf, "{s}{s}", .{ nonce_prefix, nonce });
}

fn storeNonce(queue: *queue_redis.Client, cfg: Config, nonce: []const u8) !void {
    var key_buf: [NONCE_KEY_BUF_LEN]u8 = undefined;
    try queue.setEx(try nonceKey(&key_buf, cfg.nonce_prefix, nonce), "1", cfg.ttl_seconds);
}

/// Atomic single-use: DEL returns the count removed (1 = first use, 0 = already
/// consumed or expired).
fn consumeNonce(queue: *queue_redis.Client, cfg: Config, nonce: []const u8) bool {
    var key_buf: [NONCE_KEY_BUF_LEN]u8 = undefined;
    const key = nonceKey(&key_buf, cfg.nonce_prefix, nonce) catch return false;
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
const T_CFG = Config{ .domain_prefix = "test-conn:v1:", .nonce_prefix = "connect:test:nonce:" };
const T_CFG_OTHER = Config{ .domain_prefix = "other-conn:v1:", .nonce_prefix = "connect:other:nonce:" };
const T_SECRET = "test-connect-signing-secret";
const T_WS = "0195b4ba-8d3a-7f13-8abc-2b3e1e0ddd01";
const T_NONCE = "deadbeefdeadbeefdeadbeefdeadbeef";
const T_FAR_FUTURE: i64 = 32_503_680_000_000; // year 3000, ms

test "signState/verifySignedState: round-trips workspace_id + nonce" {
    const st = try signState(testing.allocator, T_CFG, T_SECRET, T_WS, T_NONCE, T_FAR_FUTURE);
    defer testing.allocator.free(st);
    const p = verifySignedState(testing.allocator, T_CFG, T_SECRET, st, 0).?;
    defer testing.allocator.free(p.buf);
    try testing.expectEqualStrings(T_WS, p.workspace_id);
    try testing.expectEqualStrings(T_NONCE, p.nonce);
}

test "verifySignedState: rejects a tampered mac" {
    const st = try signState(testing.allocator, T_CFG, T_SECRET, T_WS, T_NONCE, T_FAR_FUTURE);
    defer testing.allocator.free(st);
    const bad = try testing.allocator.dupe(u8, st);
    defer testing.allocator.free(bad);
    bad[bad.len - 1] = if (bad[bad.len - 1] == 'a') 'b' else 'a'; // flip a mac hex digit
    try testing.expect(verifySignedState(testing.allocator, T_CFG, T_SECRET, bad, 0) == null);
}

test "verifySignedState: rejects a foreign secret" {
    const st = try signState(testing.allocator, T_CFG, T_SECRET, T_WS, T_NONCE, T_FAR_FUTURE);
    defer testing.allocator.free(st);
    try testing.expect(verifySignedState(testing.allocator, T_CFG, "other-secret", st, 0) == null);
}

test "verifySignedState: rejects an expired state" {
    const st = try signState(testing.allocator, T_CFG, T_SECRET, T_WS, T_NONCE, 1000);
    defer testing.allocator.free(st);
    try testing.expect(verifySignedState(testing.allocator, T_CFG, T_SECRET, st, 2000) == null);
}

test "verifySignedState: rejects malformed input (no separator)" {
    try testing.expect(verifySignedState(testing.allocator, T_CFG, T_SECRET, "not-a-state", 0) == null);
}

test "verifySignedState: a state minted for one connector fails another (domain separation)" {
    const st = try signState(testing.allocator, T_CFG, T_SECRET, T_WS, T_NONCE, T_FAR_FUTURE);
    defer testing.allocator.free(st);
    // Same secret + payload, different connector domain prefix → verify must fail.
    try testing.expect(verifySignedState(testing.allocator, T_CFG_OTHER, T_SECRET, st, 0) == null);
}
