//! Standard OIDC claim validation (sub, iss, aud, exp) over a decoded JWT
//! payload. Extracted from jwks.zig; re-exported there for API stability.

const std = @import("std");
const clock = @import("common").clock;
const jwks_types = @import("jwks_types.zig");

const VerifyError = jwks_types.VerifyError;
const VerifiedClaims = jwks_types.VerifiedClaims;

pub fn parseStandardClaims(
    alloc: std.mem.Allocator,
    payload_raw: []u8,
    expected_issuer: ?[]const u8,
    expected_audience: ?[]const u8,
) !VerifiedClaims {
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, payload_raw, .{}) catch return VerifyError.TokenMalformed;
    defer parsed.deinit();

    if (parsed.value != .object) return VerifyError.TokenMalformed;
    const obj = parsed.value.object;

    const subject = getString(obj, "sub") orelse return VerifyError.MissingSubject;
    const issuer = getString(obj, "iss") orelse return VerifyError.MissingIssuer;

    if (expected_issuer) |want| {
        if (!std.mem.eql(u8, issuer, want)) return VerifyError.IssuerMismatch;
    }

    if (expected_audience) |want_aud| {
        if (!audienceMatches(obj, want_aud)) return VerifyError.AudienceMismatch;
    }

    const exp = getInt(obj, "exp") orelse return VerifyError.MissingExpiry;
    const now_s = clock.nowSeconds();
    if (exp <= now_s) return VerifyError.TokenExpired;

    return .{
        .subject = try alloc.dupe(u8, subject),
        .issuer = try alloc.dupe(u8, issuer),
        .claims_json = payload_raw,
    };
}

pub fn getString(obj: std.json.ObjectMap, key: []const u8) ?[]const u8 {
    const value = obj.get(key) orelse return null;
    return switch (value) {
        .string => |s| s,
        else => null,
    };
}

pub fn getInt(obj: std.json.ObjectMap, key: []const u8) ?i64 {
    const value = obj.get(key) orelse return null;
    return switch (value) {
        .integer => |i| i,
        else => null,
    };
}

fn audienceMatches(obj: std.json.ObjectMap, wanted: []const u8) bool {
    const aud = obj.get("aud") orelse return false;
    switch (aud) {
        .string => |value| return std.mem.eql(u8, value, wanted),
        .array => |arr| {
            for (arr.items) |item| {
                if (item == .string and std.mem.eql(u8, item.string, wanted)) return true;
            }
            return false;
        },
        else => return false,
    }
}
