//! Wire-protocol surface for `session_store_redis.zig`:
//!
//!  - The three embedded Lua scripts (`VERIFY_AND_CONSUME_LUA` comes from a
//!    sibling `.lua` so authors get real syntax highlighting; the two
//!    smaller scripts stay inline because the bytes are short and
//!    semantic-context lives next to their callers).
//!  - The Zig-side types (`VerifyOutcome`, `VerifyPayload`, `Error`,
//!    `ScanReply`) that callers see.
//!  - The pure parsers that turn a Redis `RespValue` into one of those
//!    types. Pure functions: trivially testable, no `*SessionStore`.

const std = @import("std");
const redis_protocol = @import("../queue/redis_protocol.zig");

pub const VERIFY_AND_CONSUME_LUA: []const u8 = @embedFile("session_verify_consume.lua");

/// Atomic approve: pending → verification_pending in one EVAL so two
/// dashboard Approve clicks race-free (second one returns 409).
pub const APPROVE_LUA: []const u8 =
    \\local blob = redis.call("GET", KEYS[1])
    \\if not blob then return {"missing"} end
    \\local s = cjson.decode(blob)
    \\if s.status ~= "pending" then return {"conflict", s.status} end
    \\s.status = "verification_pending"
    \\s.dashboard_public_key = ARGV[1]
    \\s.ciphertext = ARGV[2]
    \\s.nonce = ARGV[3]
    \\s.verification_code_hmac_hex = ARGV[4]
    \\s.clerk_user_id = ARGV[5]
    \\s.approved_at_ms = tonumber(ARGV[6])
    \\redis.call("SET", KEYS[1], cjson.encode(s), "EX", tonumber(ARGV[7]))
    \\return {"ok"}
;

/// Owner-checked abort in one EVAL: `DELETE /sessions/{id}` transitions to
/// `aborted/explicit_cancel` only when the requesting user matches the
/// stored `clerk_user_id`.
pub const DELETE_OWNER_LUA: []const u8 =
    \\local blob = redis.call("GET", KEYS[1])
    \\if not blob then return {"missing"} end
    \\local s = cjson.decode(blob)
    \\if s.clerk_user_id ~= ARGV[1] then return {"not_owner"} end
    \\if s.status == "consumed" then return {"consumed"} end
    \\if s.status == "aborted" then return {"already_aborted"} end
    \\s.status = "aborted"
    \\s.aborted_reason = ARGV[2]
    \\redis.call("SET", KEYS[1], cjson.encode(s), "EX", tonumber(ARGV[3]))
    \\return {"ok"}
;

pub const VerifyOutcome = union(enum) {
    success: VerifyPayload,
    replay: VerifyPayload,
    invalid_code: u8,
    not_approved: void,
    aborted: []const u8,
    consumed: void,
    expired: void,
    missing: void,

    /// Deep-copy slice payloads with `alloc` so the returned outcome
    /// survives the originating RespValue's deinit. The borrowed form
    /// (returned by `parseVerifyOutcome`) is unsafe to retain after
    /// `resp.deinit` — every production caller MUST `dupe` first and
    /// `deinit` the result.
    pub fn dupe(self: VerifyOutcome, alloc: std.mem.Allocator) error{OutOfMemory}!VerifyOutcome {
        return switch (self) {
            .success => |p| .{ .success = try dupePayload(alloc, p) },
            .replay => |p| .{ .replay = try dupePayload(alloc, p) },
            .aborted => |reason| .{ .aborted = try alloc.dupe(u8, reason) },
            .invalid_code => |attempts| .{ .invalid_code = attempts },
            .not_approved, .consumed, .expired, .missing => self,
        };
    }

    pub fn deinit(self: *VerifyOutcome, alloc: std.mem.Allocator) void {
        switch (self.*) {
            .success => |p| deinitPayload(alloc, p),
            .replay => |p| deinitPayload(alloc, p),
            .aborted => |reason| alloc.free(reason),
            else => {},
        }
    }
};

pub const VerifyPayload = struct {
    dashboard_public_key: []const u8,
    ciphertext: []const u8,
    nonce: []const u8,
};

fn dupePayload(alloc: std.mem.Allocator, p: VerifyPayload) error{OutOfMemory}!VerifyPayload {
    const dpk = try alloc.dupe(u8, p.dashboard_public_key);
    errdefer alloc.free(dpk);
    const ct = try alloc.dupe(u8, p.ciphertext);
    errdefer alloc.free(ct);
    const nonce = try alloc.dupe(u8, p.nonce);
    return .{ .dashboard_public_key = dpk, .ciphertext = ct, .nonce = nonce };
}

fn deinitPayload(alloc: std.mem.Allocator, p: VerifyPayload) void {
    alloc.free(p.dashboard_public_key);
    alloc.free(p.ciphertext);
    alloc.free(p.nonce);
}

pub const Error = error{
    InvalidPublicKey,
    InvalidTokenName,
    InvalidCipherText,
    InvalidNonce,
    InvalidVerificationCode,
    AlreadyApproved,
    SessionMissing,
    NotOwner,
    SessionConsumed,
    SessionAborted,
    RedisError,
    UnexpectedRedisReply,
    OutOfMemory,
};

pub const ScanReply = struct { next_cursor: []const u8, keys: []const []const u8 };

/// SCAN reply is a 2-element array: `[cursor, [keys...]]`. Bounded into a
/// caller-provided buffer to avoid allocating per page; 128 keys per batch
/// is enough for COUNT 100 + Redis's right to return slightly more.
pub fn readScanReply(resp: redis_protocol.RespValue, keys_out: [][]const u8) !ScanReply {
    const arr = switch (resp) {
        .array => |a| a orelse return Error.UnexpectedRedisReply,
        else => return Error.UnexpectedRedisReply,
    };
    if (arr.len != 2) return Error.UnexpectedRedisReply;
    const cursor = redis_protocol.valueAsString(arr[0]) orelse return Error.UnexpectedRedisReply;
    const keys_arr = switch (arr[1]) {
        .array => |a| a orelse return Error.UnexpectedRedisReply,
        else => return Error.UnexpectedRedisReply,
    };
    if (keys_arr.len > keys_out.len) return Error.UnexpectedRedisReply;
    for (keys_arr, 0..) |item, i| {
        keys_out[i] = redis_protocol.valueAsString(item) orelse return Error.UnexpectedRedisReply;
    }
    return .{ .next_cursor = cursor, .keys = keys_out[0..keys_arr.len] };
}

pub fn firstTag(resp: redis_protocol.RespValue) ?[]const u8 {
    const arr = switch (resp) {
        .array => |a| a orelse return null,
        else => return null,
    };
    if (arr.len == 0) return null;
    return redis_protocol.valueAsString(arr[0]);
}

pub fn mapApproveOutcome(resp: redis_protocol.RespValue) Error!void {
    const tag = firstTag(resp) orelse return Error.UnexpectedRedisReply;
    if (std.mem.eql(u8, tag, "ok")) return;
    if (std.mem.eql(u8, tag, "missing")) return Error.SessionMissing;
    if (std.mem.eql(u8, tag, "conflict")) return Error.AlreadyApproved;
    return Error.UnexpectedRedisReply;
}

/// Fresh abort (`ok`) vs idempotent re-delete of an already-aborted session (`already_aborted`) — lets the handler emit the abort audit record only once.
pub const DeleteOutcome = enum { aborted, already_aborted };

pub fn mapDeleteOutcome(resp: redis_protocol.RespValue) Error!DeleteOutcome {
    const tag = firstTag(resp) orelse return Error.UnexpectedRedisReply;
    if (std.mem.eql(u8, tag, "ok")) return .aborted;
    if (std.mem.eql(u8, tag, "already_aborted")) return .already_aborted;
    if (std.mem.eql(u8, tag, "missing")) return Error.SessionMissing;
    if (std.mem.eql(u8, tag, "not_owner")) return Error.NotOwner;
    if (std.mem.eql(u8, tag, "consumed")) return Error.SessionConsumed;
    return Error.UnexpectedRedisReply;
}

pub fn parseVerifyOutcome(resp: redis_protocol.RespValue) Error!VerifyOutcome {
    const arr = switch (resp) {
        .array => |a| a orelse return Error.UnexpectedRedisReply,
        else => return Error.UnexpectedRedisReply,
    };
    if (arr.len == 0) return Error.UnexpectedRedisReply;
    const tag = redis_protocol.valueAsString(arr[0]) orelse return Error.UnexpectedRedisReply;

    if (std.mem.eql(u8, tag, "missing")) return .{ .missing = {} };
    if (std.mem.eql(u8, tag, "expired")) return .{ .expired = {} };
    if (std.mem.eql(u8, tag, "consumed")) return .{ .consumed = {} };
    if (std.mem.eql(u8, tag, "not_approved")) return .{ .not_approved = {} };
    if (std.mem.eql(u8, tag, "aborted")) {
        if (arr.len < 2) return Error.UnexpectedRedisReply;
        const reason = redis_protocol.valueAsString(arr[1]) orelse return Error.UnexpectedRedisReply;
        return .{ .aborted = reason };
    }
    if (std.mem.eql(u8, tag, "invalid_code")) {
        if (arr.len < 2) return Error.UnexpectedRedisReply;
        const s = redis_protocol.valueAsString(arr[1]) orelse return Error.UnexpectedRedisReply;
        const attempts = std.fmt.parseInt(u8, s, 10) catch return Error.UnexpectedRedisReply;
        return .{ .invalid_code = attempts };
    }
    if (std.mem.eql(u8, tag, "success") or std.mem.eql(u8, tag, "replay")) {
        if (arr.len < 4) return Error.UnexpectedRedisReply;
        const dpk = redis_protocol.valueAsString(arr[1]) orelse return Error.UnexpectedRedisReply;
        const ct = redis_protocol.valueAsString(arr[2]) orelse return Error.UnexpectedRedisReply;
        const nonce = redis_protocol.valueAsString(arr[3]) orelse return Error.UnexpectedRedisReply;
        const payload = VerifyPayload{ .dashboard_public_key = dpk, .ciphertext = ct, .nonce = nonce };
        return if (std.mem.eql(u8, tag, "success")) .{ .success = payload } else .{ .replay = payload };
    }
    return Error.UnexpectedRedisReply;
}

// ── Tests ────────────────────────────────────────────────────────────────

const testing = std.testing;

fn bulk(alloc: std.mem.Allocator, s: []const u8) !redis_protocol.RespValue {
    return .{ .bulk = try alloc.dupe(u8, s) };
}

fn arrayOf(alloc: std.mem.Allocator, items: []const []const u8) !redis_protocol.RespValue {
    var arr = try alloc.alloc(redis_protocol.RespValue, items.len);
    errdefer alloc.free(arr);
    for (items, 0..) |s, i| arr[i] = try bulk(alloc, s);
    return .{ .array = arr };
}

test "firstTag returns the leading bulk on a string-only array" {
    var resp = try arrayOf(testing.allocator, &.{ "ok", "ignored" });
    defer resp.deinit(testing.allocator);
    try testing.expectEqualStrings("ok", firstTag(resp).?);
}

test "firstTag returns null on non-array RESP" {
    const resp = redis_protocol.RespValue{ .integer = 7 };
    try testing.expect(firstTag(resp) == null);
}

test "mapApproveOutcome ok maps to void success" {
    var resp = try arrayOf(testing.allocator, &.{"ok"});
    defer resp.deinit(testing.allocator);
    try mapApproveOutcome(resp);
}

test "mapApproveOutcome conflict maps to AlreadyApproved" {
    var resp = try arrayOf(testing.allocator, &.{ "conflict", "verification_pending" });
    defer resp.deinit(testing.allocator);
    try testing.expectError(Error.AlreadyApproved, mapApproveOutcome(resp));
}

test "mapDeleteOutcome not_owner maps to NotOwner" {
    var resp = try arrayOf(testing.allocator, &.{"not_owner"});
    defer resp.deinit(testing.allocator);
    try testing.expectError(Error.NotOwner, mapDeleteOutcome(resp));
}

test "parseVerifyOutcome missing → .missing" {
    var resp = try arrayOf(testing.allocator, &.{"missing"});
    defer resp.deinit(testing.allocator);
    const outcome = try parseVerifyOutcome(resp);
    try testing.expect(outcome == .missing);
}

test "parseVerifyOutcome aborted carries reason string" {
    var resp = try arrayOf(testing.allocator, &.{ "aborted", "rate_limit_exceeded" });
    defer resp.deinit(testing.allocator);
    const outcome = try parseVerifyOutcome(resp);
    try testing.expectEqualStrings("rate_limit_exceeded", outcome.aborted);
}

test "parseVerifyOutcome invalid_code parses attempts" {
    var resp = try arrayOf(testing.allocator, &.{ "invalid_code", "3" });
    defer resp.deinit(testing.allocator);
    const outcome = try parseVerifyOutcome(resp);
    try testing.expectEqual(@as(u8, 3), outcome.invalid_code);
}

test "parseVerifyOutcome success carries payload triple" {
    var resp = try arrayOf(testing.allocator, &.{ "success", "DASH", "CIPHER", "NONCE" });
    defer resp.deinit(testing.allocator);
    const outcome = try parseVerifyOutcome(resp);
    try testing.expectEqualStrings("DASH", outcome.success.dashboard_public_key);
    try testing.expectEqualStrings("CIPHER", outcome.success.ciphertext);
    try testing.expectEqualStrings("NONCE", outcome.success.nonce);
}

test "parseVerifyOutcome replay shares the success payload shape" {
    var resp = try arrayOf(testing.allocator, &.{ "replay", "D", "C", "N" });
    defer resp.deinit(testing.allocator);
    const outcome = try parseVerifyOutcome(resp);
    try testing.expectEqualStrings("D", outcome.replay.dashboard_public_key);
}

test "VerifyOutcome.dupe survives RespValue free — pins the borrowed-slice fix" {
    // The hazard: parseVerifyOutcome returns slices borrowed from `resp`.
    // If the caller frees `resp` before reading the outcome, those slices
    // dangle. testing.allocator scribbles 0xaa on free, so this test will
    // observe corrupted bytes if dupe is missing or wrong.
    var resp = try arrayOf(testing.allocator, &.{ "success", "DASH-KEY", "CIPHER-PAYLOAD", "NONCE-BYTES" });
    const borrowed = try parseVerifyOutcome(resp);
    var owned = try borrowed.dupe(testing.allocator);
    defer owned.deinit(testing.allocator);

    // Free the RespValue first — this is the production lifecycle.
    resp.deinit(testing.allocator);

    // The owned outcome must still read the original bytes, not the
    // allocator scribble pattern.
    try testing.expectEqualStrings("DASH-KEY", owned.success.dashboard_public_key);
    try testing.expectEqualStrings("CIPHER-PAYLOAD", owned.success.ciphertext);
    try testing.expectEqualStrings("NONCE-BYTES", owned.success.nonce);
}

test "VerifyOutcome.dupe survives RespValue free — aborted reason variant" {
    var resp = try arrayOf(testing.allocator, &.{ "aborted", "rate_limit_exceeded" });
    const borrowed = try parseVerifyOutcome(resp);
    var owned = try borrowed.dupe(testing.allocator);
    defer owned.deinit(testing.allocator);

    resp.deinit(testing.allocator);
    try testing.expectEqualStrings("rate_limit_exceeded", owned.aborted);
}

test "VerifyOutcome.dupe is identity for payload-less variants" {
    inline for (.{ "consumed", "expired", "missing", "not_approved" }) |tag| {
        var resp = try arrayOf(testing.allocator, &.{tag});
        defer resp.deinit(testing.allocator);
        const borrowed = try parseVerifyOutcome(resp);
        var owned = try borrowed.dupe(testing.allocator);
        defer owned.deinit(testing.allocator); // no-op; pin it doesn't crash.
        try testing.expect(std.meta.activeTag(owned) == std.meta.activeTag(borrowed));
    }
}

test "VerifyOutcome.dupe preserves invalid_code u8 attempts" {
    var resp = try arrayOf(testing.allocator, &.{ "invalid_code", "4" });
    defer resp.deinit(testing.allocator);
    const borrowed = try parseVerifyOutcome(resp);
    var owned = try borrowed.dupe(testing.allocator);
    defer owned.deinit(testing.allocator);
    try testing.expectEqual(@as(u8, 4), owned.invalid_code);
}

test "VERIFY_AND_CONSUME_LUA embed is non-empty and references attempts" {
    try testing.expect(VERIFY_AND_CONSUME_LUA.len > 100);
    try testing.expect(std.mem.indexOf(u8, VERIFY_AND_CONSUME_LUA, "verification_attempts") != null);
}

test "APPROVE_LUA decodes JSON and writes verification_pending" {
    try testing.expect(std.mem.indexOf(u8, APPROVE_LUA, "cjson.decode") != null);
    try testing.expect(std.mem.indexOf(u8, APPROVE_LUA, "verification_pending") != null);
}
