//! Provider-specific claim extraction from verified JWT payloads.
//! Currently supports Clerk. Add new providers as additional extract functions.

const std = @import("std");
const jwks = @import("jwks.zig");

pub const ClerkClaims = struct {
    tenant_id: ?[]u8,
    org_id: ?[]u8,
};

/// Extract Clerk-specific claims from a verified JWT payload.
/// Looks for `org_id` at top level and `tenant_id` at top level
/// or nested under `metadata.tenant_id`.
pub fn extractClerkClaims(alloc: std.mem.Allocator, claims_json: []const u8) !ClerkClaims {
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, claims_json, .{}) catch
        return jwks.VerifyError.TokenMalformed;
    defer parsed.deinit();

    if (parsed.value != .object) return jwks.VerifyError.TokenMalformed;
    const obj = parsed.value.object;

    const org_id = jwks.getString(obj, "org_id");
    const tenant_id = getClerkTenantId(obj);

    return .{
        .tenant_id = if (tenant_id) |v| try alloc.dupe(u8, v) else null,
        .org_id = if (org_id) |v| try alloc.dupe(u8, v) else null,
    };
}

fn getClerkTenantId(obj: std.json.ObjectMap) ?[]const u8 {
    if (jwks.getString(obj, "tenant_id")) |v| return v;

    const metadata = obj.get("metadata") orelse return null;
    if (metadata != .object) return null;
    return jwks.getString(metadata.object, "tenant_id");
}

test "extractClerkClaims from metadata.tenant_id" {
    const json =
        \\{"sub":"user_1","iss":"https://clerk.example.com","exp":9999999999,"org_id":"org_1","metadata":{"tenant_id":"tenant_a"}}
    ;
    const result = try extractClerkClaims(std.testing.allocator, json);
    defer {
        if (result.tenant_id) |v| std.testing.allocator.free(v);
        if (result.org_id) |v| std.testing.allocator.free(v);
    }
    try std.testing.expectEqualStrings("tenant_a", result.tenant_id.?);
    try std.testing.expectEqualStrings("org_1", result.org_id.?);
}

test "extractClerkClaims from top-level tenant_id" {
    const json =
        \\{"sub":"user_1","iss":"https://clerk.example.com","exp":9999999999,"tenant_id":"tenant_b"}
    ;
    const result = try extractClerkClaims(std.testing.allocator, json);
    defer {
        if (result.tenant_id) |v| std.testing.allocator.free(v);
        if (result.org_id) |v| std.testing.allocator.free(v);
    }
    try std.testing.expectEqualStrings("tenant_b", result.tenant_id.?);
    try std.testing.expect(result.org_id == null);
}

test "extractClerkClaims with no tenant or org" {
    const json =
        \\{"sub":"user_1","iss":"https://clerk.example.com","exp":9999999999}
    ;
    const result = try extractClerkClaims(std.testing.allocator, json);
    defer {
        if (result.tenant_id) |v| std.testing.allocator.free(v);
        if (result.org_id) |v| std.testing.allocator.free(v);
    }
    try std.testing.expect(result.tenant_id == null);
    try std.testing.expect(result.org_id == null);
}

test "extractClerkClaims rejects non-JSON" {
    try std.testing.expectError(jwks.VerifyError.TokenMalformed, extractClerkClaims(std.testing.allocator, "not json"));
}

test "extractClerkClaims rejects non-object JSON" {
    try std.testing.expectError(jwks.VerifyError.TokenMalformed, extractClerkClaims(std.testing.allocator, "[1,2,3]"));
}
