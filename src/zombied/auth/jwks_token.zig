const std = @import("std");
const jwks_types = @import("jwks_types.zig");

const VerifyError = jwks_types.VerifyError;
const JwtParts = jwks_types.JwtParts;

pub fn extractBearerToken(authorization: []const u8) ![]const u8 {
    const prefix = "Bearer ";
    if (!std.mem.startsWith(u8, authorization, prefix)) return VerifyError.InvalidAuthorization;
    const token = std.mem.trim(u8, authorization[prefix.len..], " \t");
    if (token.len == 0) return VerifyError.InvalidAuthorization;
    return token;
}

pub fn splitJwt(token: []const u8) !JwtParts {
    var split = std.mem.splitScalar(u8, token, '.');
    const a = split.next() orelse return VerifyError.TokenMalformed;
    const b = split.next() orelse return VerifyError.TokenMalformed;
    const c = split.next() orelse return VerifyError.TokenMalformed;
    if (split.next() != null) return VerifyError.TokenMalformed;
    if (a.len == 0 or b.len == 0 or c.len == 0) return VerifyError.TokenMalformed;
    return .{ .header_b64 = a, .payload_b64 = b, .signature_b64 = c };
}

pub fn decodeBase64UrlOwned(alloc: std.mem.Allocator, text: []const u8) ![]u8 {
    const size = std.base64.url_safe_no_pad.Decoder.calcSizeForSlice(text) catch return VerifyError.TokenMalformed;
    const out = try alloc.alloc(u8, size);
    errdefer alloc.free(out);
    std.base64.url_safe_no_pad.Decoder.decode(out, text) catch return VerifyError.TokenMalformed;
    return out;
}
