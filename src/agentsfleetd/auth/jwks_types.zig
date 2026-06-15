const std = @import("std");

pub const VerifyError = error{
    MissingAuthorization,
    InvalidAuthorization,
    TokenMalformed,
    UnsupportedAlgorithm,
    MissingKeyId,
    MissingSubject,
    MissingIssuer,
    MissingExpiry,
    TokenExpired,
    IssuerMismatch,
    AudienceMismatch,
    JwksFetchFailed,
    JwksParseFailed,
    JwkNotFound,
    SignatureInvalid,
};

pub const VerifiedClaims = struct {
    subject: []u8,
    issuer: []u8,
    claims_json: []u8,
};

pub const JwtParts = struct {
    header_b64: []const u8,
    payload_b64: []const u8,
    signature_b64: []const u8,
};

pub const JwkKey = struct {
    kid: []u8,
    modulus: []u8,
    exponent: []u8,
};

pub const JwksCache = struct {
    const Self = @This();

    fetched_at_ms: i64,
    keys: []JwkKey,

    pub fn deinit(self: *Self, alloc: std.mem.Allocator) void {
        for (self.keys) |key| {
            alloc.free(key.kid);
            alloc.free(key.modulus);
            alloc.free(key.exponent);
        }
        alloc.free(self.keys);
    }
};
