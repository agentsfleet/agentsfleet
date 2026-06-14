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
const CONSUMED = "consumed";
const MISSING = "missing";
const STATUS_OK = "ok";
const SUCCESS = "success";
const LUA_MS_PER_SECOND = "1000";

pub const VERIFY_AND_CONSUME_LUA: []const u8 = @embedFile("session_verify_consume.lua");

/// Atomic approve: pending → verification_pending in one EVAL so two
/// dashboard Approve clicks race-free (second one returns 409). Re-stamps
/// `expires_at_ms` to match the reset TTL — otherwise the stale create-time
/// expiry would let a background sweep prune a freshly-approved session.
pub const APPROVE_LUA: []const u8 =
    \\local blob = redis.call("GET", KEYS[1])
    \\if not blob then return {"
++ MISSING ++
    \\"} end
    \\local s = cjson.decode(blob)
    \\if s.status ~= "pending" then return {"conflict", s.status} end
    \\s.status = "verification_pending"
    \\s.dashboard_public_key = ARGV[1]
    \\s.ciphertext = ARGV[2]
    \\s.nonce = ARGV[3]
    \\s.verification_code_hmac_hex = ARGV[4]
    \\s.clerk_user_id = ARGV[5]
    \\s.approved_at_ms = tonumber(ARGV[6])
    \\s.expires_at_ms = tonumber(ARGV[6]) + tonumber(ARGV[7]) *
++ LUA_MS_PER_SECOND ++
    \\
    \\redis.call("SET", KEYS[1], cjson.encode(s), "EX", tonumber(ARGV[7]))
    \\return {"
++ STATUS_OK ++
    \\"}
;

/// Owner-checked abort in one EVAL: `DELETE /sessions/{id}` transitions to
/// `aborted/explicit_cancel` only when the requesting user matches the
/// stored `clerk_user_id`.
pub const DELETE_OWNER_LUA: []const u8 =
    \\local blob = redis.call("GET", KEYS[1])
    \\if not blob then return {"
++ MISSING ++
    \\"} end
    \\local s = cjson.decode(blob)
    \\if s.clerk_user_id ~= ARGV[1] then return {"not_owner"} end
    \\if s.status == "
++ CONSUMED ++
    \\" then return {"
++ CONSUMED ++
    \\"} end
    \\if s.status == "aborted" then return {"already_aborted"} end
    \\s.status = "aborted"
    \\s.aborted_reason = ARGV[2]
    \\redis.call("SET", KEYS[1], cjson.encode(s), "EX", tonumber(ARGV[3]))
    \\return {"
++ STATUS_OK ++
    \\"}
;

pub const VerifyOutcome = union(enum) {
    success: VerifyPayload,
    replay: VerifyPayload,
    invalid_code: u8,
    not_approved: void,
    aborted: []const u8,
    // The wrong attempt that just tripped MAX_VERIFY_ATTEMPTS. Distinct from
    // `aborted` (a re-verify of an already-terminal session) so the handler
    // emits the lockout audit exactly once, on the transition.
    rate_limited: void,
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
            .not_approved, .rate_limited, .consumed, .expired, .missing => self,
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
    if (std.mem.eql(u8, tag, STATUS_OK)) return;
    if (std.mem.eql(u8, tag, MISSING)) return Error.SessionMissing;
    if (std.mem.eql(u8, tag, "conflict")) return Error.AlreadyApproved;
    return Error.UnexpectedRedisReply;
}

/// Fresh abort (`ok`) vs idempotent re-delete of an already-aborted session (`already_aborted`) — lets the handler emit the abort audit record only once.
pub const DeleteOutcome = enum { aborted, already_aborted };

pub fn mapDeleteOutcome(resp: redis_protocol.RespValue) Error!DeleteOutcome {
    const tag = firstTag(resp) orelse return Error.UnexpectedRedisReply;
    if (std.mem.eql(u8, tag, STATUS_OK)) return .aborted;
    if (std.mem.eql(u8, tag, "already_aborted")) return .already_aborted;
    if (std.mem.eql(u8, tag, MISSING)) return Error.SessionMissing;
    if (std.mem.eql(u8, tag, "not_owner")) return Error.NotOwner;
    if (std.mem.eql(u8, tag, CONSUMED)) return Error.SessionConsumed;
    return Error.UnexpectedRedisReply;
}

pub fn parseVerifyOutcome(resp: redis_protocol.RespValue) Error!VerifyOutcome {
    const arr = switch (resp) {
        .array => |a| a orelse return Error.UnexpectedRedisReply,
        else => return Error.UnexpectedRedisReply,
    };
    if (arr.len == 0) return Error.UnexpectedRedisReply;
    const tag = redis_protocol.valueAsString(arr[0]) orelse return Error.UnexpectedRedisReply;

    if (std.mem.eql(u8, tag, MISSING)) return .{ .missing = {} };
    if (std.mem.eql(u8, tag, "expired")) return .{ .expired = {} };
    if (std.mem.eql(u8, tag, CONSUMED)) return .{ .consumed = {} };
    if (std.mem.eql(u8, tag, "not_approved")) return .{ .not_approved = {} };
    if (std.mem.eql(u8, tag, "rate_limited")) return .{ .rate_limited = {} };
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
    if (std.mem.eql(u8, tag, SUCCESS) or std.mem.eql(u8, tag, "replay")) {
        if (arr.len < 4) return Error.UnexpectedRedisReply;
        const dpk = redis_protocol.valueAsString(arr[1]) orelse return Error.UnexpectedRedisReply;
        const ct = redis_protocol.valueAsString(arr[2]) orelse return Error.UnexpectedRedisReply;
        const nonce = redis_protocol.valueAsString(arr[3]) orelse return Error.UnexpectedRedisReply;
        const payload = VerifyPayload{ .dashboard_public_key = dpk, .ciphertext = ct, .nonce = nonce };
        return if (std.mem.eql(u8, tag, SUCCESS)) .{ .success = payload } else .{ .replay = payload };
    }
    return Error.UnexpectedRedisReply;
}
