const std = @import("std");
const hmac_sig = @import("hmac_sig");

const QStashVerifier = @import("QStashVerifier.zig");

const DESTINATION = "https://api.agentsfleet.net/v1/ingress/qstash/schedules";
const CURRENT_KEY = "current-signing-key";
const NEXT_KEY = "next-signing-key";
const NOW: i64 = 1_800_000_000;
const MESSAGE_ID = "msg_105_001";

const Claims = struct {
    issuer: []const u8 = "Upstash",
    subject: []const u8 = DESTINATION,
    expires_at: i64 = NOW + 60,
    not_before: i64 = NOW - 1,
    issued_at: i64 = NOW - 1,
    message_id: []const u8 = MESSAGE_ID,
    body: []const u8 = "{\"schedule_id\":\"schedule\",\"generation\":7}",
    padded_body_hash: bool = false,
};

fn encodeOwned(alloc: std.mem.Allocator, raw: []const u8) ![]u8 {
    const len = std.base64.url_safe_no_pad.Encoder.calcSize(raw.len);
    const encoded = try alloc.alloc(u8, len);
    _ = std.base64.url_safe_no_pad.Encoder.encode(encoded, raw);
    return encoded;
}

fn bodyHash(alloc: std.mem.Allocator, body: []const u8) ![]u8 {
    var digest: [std.crypto.hash.sha2.Sha256.digest_length]u8 = undefined;
    std.crypto.hash.sha2.Sha256.hash(body, &digest, .{});
    return encodeOwned(alloc, &digest);
}

fn sign(alloc: std.mem.Allocator, key: []const u8, claims: Claims) ![]u8 {
    const header = try encodeOwned(alloc, "{\"alg\":\"HS256\",\"typ\":\"JWT\"}");
    defer alloc.free(header);
    const hash = try bodyHash(alloc, claims.body);
    defer alloc.free(hash);
    const padded_hash = if (claims.padded_body_hash)
        try std.fmt.allocPrint(alloc, "{s}=", .{hash})
    else
        null;
    defer if (padded_hash) |value| alloc.free(value);
    const payload_json = try std.json.Stringify.valueAlloc(alloc, .{
        .iss = claims.issuer,
        .sub = claims.subject,
        .exp = claims.expires_at,
        .nbf = claims.not_before,
        .iat = claims.issued_at,
        .jti = claims.message_id,
        .body = padded_hash orelse hash,
    }, .{});
    defer alloc.free(payload_json);
    const payload = try encodeOwned(alloc, payload_json);
    defer alloc.free(payload);
    const mac = hmac_sig.computeMac(key, &.{ header, ".", payload });
    const signature = try encodeOwned(alloc, &mac);
    defer alloc.free(signature);
    return std.fmt.allocPrint(alloc, "{s}.{s}.{s}", .{ header, payload, signature });
}

fn expectVerified(key: []const u8, padded_body_hash: bool) !void {
    const alloc = std.testing.allocator;
    const claims: Claims = .{ .padded_body_hash = padded_body_hash };
    const token = try sign(alloc, key, claims);
    defer alloc.free(token);
    const verifier = QStashVerifier.init(DESTINATION, CURRENT_KEY, NEXT_KEY);
    var verified = try verifier.verifyAt(alloc, token, claims.body, NOW);
    defer verified.deinit(alloc);
    try std.testing.expectEqualStrings(MESSAGE_ID, verified.message_id);
}

test "qstash verifier: accepts current and next rotation keys" {
    try expectVerified(CURRENT_KEY, false);
    try expectVerified(NEXT_KEY, true);
}

test "qstash verifier: rejects claims and body mismatches" {
    const cases = [_]struct { claims: Claims, expected: QStashVerifier.VerifyError }{
        .{ .claims = .{ .issuer = "attacker" }, .expected = error.IssuerMismatch },
        .{ .claims = .{ .subject = "https://attacker.invalid" }, .expected = error.SubjectMismatch },
        .{ .claims = .{ .expires_at = NOW }, .expected = error.TokenExpired },
        .{ .claims = .{ .not_before = NOW + 1 }, .expected = error.TokenNotYetValid },
        .{ .claims = .{ .message_id = "" }, .expected = error.ClaimsInvalid },
    };
    const alloc = std.testing.allocator;
    const verifier = QStashVerifier.init(DESTINATION, CURRENT_KEY, NEXT_KEY);
    for (cases) |case| {
        const token = try sign(alloc, CURRENT_KEY, case.claims);
        defer alloc.free(token);
        try std.testing.expectError(case.expected, verifier.verifyAt(alloc, token, case.claims.body, NOW));
    }
    const claims: Claims = .{};
    const token = try sign(alloc, CURRENT_KEY, claims);
    defer alloc.free(token);
    try std.testing.expectError(error.BodyMismatch, verifier.verifyAt(alloc, token, claims.body ++ " ", NOW));
}

test "qstash verifier: verifies signature before parsing claims" {
    const alloc = std.testing.allocator;
    const header = try encodeOwned(alloc, "{\"alg\":\"HS256\"}");
    defer alloc.free(header);
    const payload = try encodeOwned(alloc, "not-json");
    defer alloc.free(payload);
    const signature = try encodeOwned(alloc, "01234567890123456789012345678901");
    defer alloc.free(signature);
    const token = try std.fmt.allocPrint(alloc, "{s}.{s}.{s}", .{ header, payload, signature });
    defer alloc.free(token);
    const verifier = QStashVerifier.init(DESTINATION, CURRENT_KEY, NEXT_KEY);
    try std.testing.expectError(error.SignatureInvalid, verifier.verifyAt(alloc, token, "{}", NOW));
}

test "qstash verifier: rejects malformed token, algorithm, and unknown key" {
    const alloc = std.testing.allocator;
    const verifier = QStashVerifier.init(DESTINATION, CURRENT_KEY, NEXT_KEY);
    try std.testing.expectError(error.TokenMalformed, verifier.verifyAt(alloc, "one.two", "{}", NOW));
    const claims: Claims = .{};
    const unknown = try sign(alloc, "unknown-key", claims);
    defer alloc.free(unknown);
    try std.testing.expectError(error.SignatureInvalid, verifier.verifyAt(alloc, unknown, claims.body, NOW));

    const header = try encodeOwned(alloc, "{\"alg\":\"none\"}");
    defer alloc.free(header);
    const token = try std.fmt.allocPrint(alloc, "{s}.e30.signature", .{header});
    defer alloc.free(token);
    try std.testing.expectError(error.UnsupportedAlgorithm, verifier.verifyAt(alloc, token, "{}", NOW));
}

test "qstash verifier: missing rotation key and oversized token fail closed" {
    const missing_key = QStashVerifier.init(DESTINATION, CURRENT_KEY, "");
    try std.testing.expectError(error.SigningKeysMissing, missing_key.verifyAt(std.testing.allocator, "x", "{}", NOW));
    const verifier = QStashVerifier.init(DESTINATION, CURRENT_KEY, NEXT_KEY);
    const oversized = "x" ** (QStashVerifier.MAX_TOKEN_BYTES + 1);
    try std.testing.expectError(error.TokenTooLarge, verifier.verifyAt(std.testing.allocator, oversized, "{}", NOW));
}

test "qstash verifier: oversized signed message identifier fails closed" {
    const alloc = std.testing.allocator;
    const oversized_message_id = "x" ** (QStashVerifier.MAX_MESSAGE_ID_BYTES + 1);
    const claims: Claims = .{ .message_id = oversized_message_id };
    const token = try sign(alloc, CURRENT_KEY, claims);
    defer alloc.free(token);
    const verifier = QStashVerifier.init(DESTINATION, CURRENT_KEY, NEXT_KEY);
    try std.testing.expectError(error.ClaimsInvalid, verifier.verifyAt(alloc, token, claims.body, NOW));
}

fn allocationSweep(alloc: std.mem.Allocator) !void {
    const claims: Claims = .{};
    const token = try sign(alloc, CURRENT_KEY, claims);
    defer alloc.free(token);
    const verifier = QStashVerifier.init(DESTINATION, CURRENT_KEY, NEXT_KEY);
    var verified = try verifier.verifyAt(alloc, token, claims.body, NOW);
    defer verified.deinit(alloc);
}

test "qstash verifier: every allocation failure unwinds without a leak" {
    try std.testing.checkAllAllocationFailures(std.testing.allocator, allocationSweep, .{});
}
