//! QStash delivery signature verification.
//!
//! Authentication completes before the delivery body is parsed by ingress.
//! Current and next vault keys are checked to permit rotation without a gap.

const QStashVerifier = @This();

const std = @import("std");
const common = @import("common");
const hmac_sig = @import("hmac_sig");

const jwt_token = @import("../auth/jwks_token.zig");

const EXPECTED_ALGORITHM = "HS256";
const EXPECTED_ISSUER = "Upstash";
pub const MAX_TOKEN_BYTES: usize = 8 * 1024;
pub const MAX_MESSAGE_ID_BYTES: usize = 256;
const MAX_CLAIMS_BYTES: usize = 4 * 1024;
const SHA256_BYTES = std.crypto.hash.sha2.Sha256.digest_length;
const BODY_HASH_BYTES = std.base64.url_safe_no_pad.Encoder.calcSize(SHA256_BYTES);

destination_url: []const u8,
current_key: []const u8,
next_key: []const u8,

pub const VerifyError = error{
    SigningKeysMissing,
    TokenTooLarge,
    TokenMalformed,
    UnsupportedAlgorithm,
    SignatureInvalid,
    ClaimsInvalid,
    IssuerMismatch,
    SubjectMismatch,
    TokenExpired,
    TokenNotYetValid,
    BodyMismatch,
};

pub const VerifiedDelivery = struct {
    message_id: []u8,

    pub fn deinit(self: *VerifiedDelivery, alloc: std.mem.Allocator) void {
        alloc.free(self.message_id);
        self.* = undefined;
    }
};

pub fn init(destination_url: []const u8, current_key: []const u8, next_key: []const u8) QStashVerifier {
    return .{
        .destination_url = destination_url,
        .current_key = current_key,
        .next_key = next_key,
    };
}

pub fn verify(
    self: QStashVerifier,
    alloc: std.mem.Allocator,
    token: []const u8,
    raw_body: []const u8,
) (VerifyError || error{OutOfMemory})!VerifiedDelivery {
    return self.verifyAt(alloc, token, raw_body, common.clock.nowSeconds());
}

pub fn verifyAt(
    self: QStashVerifier,
    alloc: std.mem.Allocator,
    token: []const u8,
    raw_body: []const u8,
    now_seconds: i64,
) (VerifyError || error{OutOfMemory})!VerifiedDelivery {
    if (self.current_key.len == 0 or self.next_key.len == 0) return VerifyError.SigningKeysMissing;
    if (token.len == 0 or token.len > MAX_TOKEN_BYTES) return VerifyError.TokenTooLarge;
    const parts = jwt_token.splitJwt(token) catch return VerifyError.TokenMalformed;
    try verifyHeader(alloc, parts.header_b64);
    try self.verifySignature(parts);

    const payload = try decodeBounded(alloc, parts.payload_b64, MAX_CLAIMS_BYTES);
    defer alloc.free(payload);
    const Claims = struct {
        iss: []const u8,
        sub: []const u8,
        exp: i64,
        nbf: i64,
        jti: []const u8,
        body: []const u8,
    };
    var parsed = std.json.parseFromSlice(Claims, alloc, payload, .{ .ignore_unknown_fields = true }) catch |err| switch (err) {
        error.OutOfMemory => return error.OutOfMemory,
        else => return VerifyError.ClaimsInvalid,
    };
    defer parsed.deinit();
    const claims = parsed.value;
    if (!std.mem.eql(u8, claims.iss, EXPECTED_ISSUER)) return VerifyError.IssuerMismatch;
    if (!std.mem.eql(u8, claims.sub, self.destination_url)) return VerifyError.SubjectMismatch;
    if (claims.exp <= now_seconds) return VerifyError.TokenExpired;
    if (claims.nbf > now_seconds) return VerifyError.TokenNotYetValid;
    if (claims.jti.len == 0 or claims.jti.len > MAX_MESSAGE_ID_BYTES) return VerifyError.ClaimsInvalid;
    if (!bodyHashMatches(raw_body, claims.body)) return VerifyError.BodyMismatch;
    return .{ .message_id = try alloc.dupe(u8, claims.jti) };
}

fn verifyHeader(alloc: std.mem.Allocator, encoded: []const u8) (VerifyError || error{OutOfMemory})!void {
    const decoded = try decodeBounded(alloc, encoded, 256);
    defer alloc.free(decoded);
    const Header = struct { alg: []const u8 };
    var parsed = std.json.parseFromSlice(Header, alloc, decoded, .{ .ignore_unknown_fields = true }) catch |err| switch (err) {
        error.OutOfMemory => return error.OutOfMemory,
        else => return VerifyError.TokenMalformed,
    };
    defer parsed.deinit();
    if (!std.mem.eql(u8, parsed.value.alg, EXPECTED_ALGORITHM)) return VerifyError.UnsupportedAlgorithm;
}

fn verifySignature(self: QStashVerifier, parts: anytype) VerifyError!void {
    var supplied: [hmac_sig.MAC_LEN]u8 = undefined;
    const decoded_len = std.base64.url_safe_no_pad.Decoder.calcSizeForSlice(parts.signature_b64) catch
        return VerifyError.TokenMalformed;
    if (decoded_len != supplied.len) return VerifyError.TokenMalformed;
    std.base64.url_safe_no_pad.Decoder.decode(&supplied, parts.signature_b64) catch
        return VerifyError.TokenMalformed;
    const current_mac = hmac_sig.computeMac(self.current_key, &.{ parts.header_b64, ".", parts.payload_b64 });
    const next_mac = hmac_sig.computeMac(self.next_key, &.{ parts.header_b64, ".", parts.payload_b64 });
    const current_valid = hmac_sig.constantTimeEql(&current_mac, &supplied);
    const next_valid = hmac_sig.constantTimeEql(&next_mac, &supplied);
    if (!current_valid and !next_valid) return VerifyError.SignatureInvalid;
}

fn decodeBounded(
    alloc: std.mem.Allocator,
    encoded: []const u8,
    max_decoded: usize,
) (VerifyError || error{OutOfMemory})![]u8 {
    const decoded_len = std.base64.url_safe_no_pad.Decoder.calcSizeForSlice(encoded) catch
        return VerifyError.TokenMalformed;
    if (decoded_len > max_decoded) return VerifyError.TokenTooLarge;
    const decoded = try alloc.alloc(u8, decoded_len);
    errdefer alloc.free(decoded);
    std.base64.url_safe_no_pad.Decoder.decode(decoded, encoded) catch return VerifyError.TokenMalformed;
    return decoded;
}

fn bodyHashMatches(raw_body: []const u8, supplied: []const u8) bool {
    var digest: [SHA256_BYTES]u8 = undefined;
    std.crypto.hash.sha2.Sha256.hash(raw_body, &digest, .{});
    var encoded: [BODY_HASH_BYTES]u8 = undefined;
    const expected = std.base64.url_safe_no_pad.Encoder.encode(&encoded, &digest);
    const normalized = if (supplied.len == expected.len + 1 and supplied[supplied.len - 1] == '=')
        supplied[0 .. supplied.len - 1]
    else
        supplied;
    return hmac_sig.constantTimeEql(expected, normalized);
}
