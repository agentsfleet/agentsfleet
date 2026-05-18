//! Pure types for the CLI device-flow session blob.
//!
//! Held at `auth:session:{session_id}` in Redis (see `session_store_redis.zig`).
//! Decoupled from any I/O so handlers can read the shape without dragging the
//! queue Pool into their dependency graph.
//!
//! All HMAC + fingerprint bytes are hex-encoded so the Lua script (which has
//! neither bit ops nor crypto across all Redis flavors) can byte-compare them
//! verbatim. Decoding back to fixed-length arrays happens in Zig via
//! `crypto/hmac_sig.hexDecode32`.

const std = @import("std");

/// Lifecycle states for a CLI auth session. Monotonic — no backward
/// transitions. Terminal states (`consumed` / `expired` / `aborted`)
/// reject every subsequent state-mutating call.
pub const SessionStatus = enum {
    pending,
    verification_pending,
    consumed,
    expired,
    aborted,

    pub fn fromString(s: []const u8) ?SessionStatus {
        const fields = @typeInfo(SessionStatus).@"enum".fields;
        inline for (fields) |f| {
            if (std.mem.eql(u8, s, f.name)) return @enumFromInt(f.value);
        }
        return null;
    }

    pub fn isTerminal(self: SessionStatus) bool {
        return switch (self) {
            .consumed, .expired, .aborted => true,
            .pending, .verification_pending => false,
        };
    }
};

/// JSON-encoded shape persisted in Redis. Field names map directly to the
/// JSON keys; `?` fields are emitted as `null` when unset.
///
/// `verification_code_hmac_hex` and `consumed_client_fingerprint_hex` are
/// hex-encoded so Lua can compare them as plain strings. Length-validate
/// (64 hex chars = 32 bytes) on read if cryptographic strength matters.
pub const SessionState = struct {
    session_id: []const u8,
    status: SessionStatus,
    cli_public_key: []const u8,
    token_name: []const u8,

    dashboard_public_key: ?[]const u8 = null,
    ciphertext: ?[]const u8 = null,
    nonce: ?[]const u8 = null,
    verification_code_hmac_hex: ?[]const u8 = null,

    verification_attempts: u8 = 0,
    created_at_ms: i64,
    expires_at_ms: i64,
    approved_at_ms: ?i64 = null,
    consumed_at_ms: ?i64 = null,
    aborted_reason: ?[]const u8 = null,
    clerk_user_id: ?[]const u8 = null,

    consumed_client_fingerprint_hex: ?[]const u8 = null,
    consume_payload_expires_at_ms: ?i64 = null,
};

pub const SESSION_TTL_MS: i64 = 5 * 60 * 1000;

/// Hex length expected for the HMAC + fingerprint fields. Lua trusts the
/// constant; Zig validates on decode-time so a corrupted blob from a
/// future-incompatible writer surfaces with `error.InvalidSessionBlob`
/// rather than a silent miscompare.
pub const HEX32_LEN: usize = 64;

/// Caller owns the returned slice; pair with `alloc.free`.
pub fn encode(alloc: std.mem.Allocator, state: SessionState) ![]u8 {
    return std.json.Stringify.valueAlloc(alloc, state, .{});
}

/// Owned parse — returned struct's strings live in the returned `Parsed`'s
/// arena until `deinit`. Caller must hold `Parsed` for the SessionState's
/// lifetime; `parsed.value` is the struct.
pub fn decode(alloc: std.mem.Allocator, blob: []const u8) !std.json.Parsed(SessionState) {
    return std.json.parseFromSlice(SessionState, alloc, blob, .{
        .allocate = .alloc_always,
        .ignore_unknown_fields = true,
    });
}

// ── Tests ────────────────────────────────────────────────────────────────

const testing = std.testing;

test "SessionStatus.fromString roundtrips every enum tag" {
    inline for (@typeInfo(SessionStatus).@"enum".fields) |f| {
        const parsed = SessionStatus.fromString(f.name).?;
        try testing.expectEqual(@as(SessionStatus, @enumFromInt(f.value)), parsed);
    }
}

test "SessionStatus.fromString rejects unknown labels" {
    try testing.expect(SessionStatus.fromString("verified") == null);
    try testing.expect(SessionStatus.fromString("") == null);
    try testing.expect(SessionStatus.fromString("PENDING") == null);
}

test "SessionStatus.isTerminal flags terminal states" {
    try testing.expect(!SessionStatus.pending.isTerminal());
    try testing.expect(!SessionStatus.verification_pending.isTerminal());
    try testing.expect(SessionStatus.consumed.isTerminal());
    try testing.expect(SessionStatus.expired.isTerminal());
    try testing.expect(SessionStatus.aborted.isTerminal());
}

test "encode then decode preserves pending session fields" {
    const state = SessionState{
        .session_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa40",
        .status = .pending,
        .cli_public_key = "BASE64URL_SPKI",
        .token_name = "kishore-laptop",
        .created_at_ms = 1_700_000_000_000,
        .expires_at_ms = 1_700_000_300_000,
    };
    const blob = try encode(testing.allocator, state);
    defer testing.allocator.free(blob);

    var parsed = try decode(testing.allocator, blob);
    defer parsed.deinit();
    try testing.expectEqual(SessionStatus.pending, parsed.value.status);
    try testing.expectEqualStrings("0195b4ba-8d3a-7f13-8abc-2b3e1e0caa40", parsed.value.session_id);
    try testing.expectEqualStrings("kishore-laptop", parsed.value.token_name);
    try testing.expectEqual(@as(u8, 0), parsed.value.verification_attempts);
    try testing.expect(parsed.value.dashboard_public_key == null);
    try testing.expect(parsed.value.consumed_at_ms == null);
}

test "encode then decode preserves verification_pending optional fields" {
    const state = SessionState{
        .session_id = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa41",
        .status = .verification_pending,
        .cli_public_key = "CLI_KEY",
        .token_name = "default",
        .dashboard_public_key = "DASH_KEY",
        .ciphertext = "CIPHER",
        .nonce = "NONCE",
        .verification_code_hmac_hex = "a" ** 64,
        .verification_attempts = 2,
        .created_at_ms = 1,
        .expires_at_ms = 2,
        .approved_at_ms = 3,
        .clerk_user_id = "user_abc",
    };
    const blob = try encode(testing.allocator, state);
    defer testing.allocator.free(blob);

    var parsed = try decode(testing.allocator, blob);
    defer parsed.deinit();
    try testing.expectEqual(SessionStatus.verification_pending, parsed.value.status);
    try testing.expectEqualStrings("a" ** 64, parsed.value.verification_code_hmac_hex.?);
    try testing.expectEqual(@as(u8, 2), parsed.value.verification_attempts);
    try testing.expectEqual(@as(i64, 3), parsed.value.approved_at_ms.?);
    try testing.expectEqualStrings("user_abc", parsed.value.clerk_user_id.?);
}

test "decode tolerates unknown fields (forward compatibility)" {
    const blob =
        \\{"session_id":"sid","status":"pending","cli_public_key":"k",
        \\ "token_name":"n","created_at_ms":1,"expires_at_ms":2,
        \\ "verification_attempts":0,"future_field":"ignored"}
    ;
    var parsed = try decode(testing.allocator, blob);
    defer parsed.deinit();
    try testing.expectEqual(SessionStatus.pending, parsed.value.status);
}

test "decode rejects unknown status values" {
    const blob =
        \\{"session_id":"sid","status":"verified","cli_public_key":"k",
        \\ "token_name":"n","created_at_ms":1,"expires_at_ms":2,
        \\ "verification_attempts":0}
    ;
    try testing.expectError(error.InvalidEnumTag, decode(testing.allocator, blob));
}

test "HEX32_LEN matches a 32-byte HMAC hex encoding" {
    try testing.expectEqual(@as(usize, 64), HEX32_LEN);
}
