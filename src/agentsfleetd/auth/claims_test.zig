//! Tests for verified-JWT claim extraction (role/platform_admin
//! claims removed; capability rides the `scopes` claim). FLL-exempt.

const std = @import("std");
const jwks = @import("jwks.zig");
const claims = @import("claims.zig");

const IdentityClaims = claims.IdentityClaims;
const extractClerkClaims = claims.extractClerkClaims;
const extractCustomClaims = claims.extractCustomClaims;

fn freeClaims(result: IdentityClaims) void {
    if (result.tenant_id) |v| std.testing.allocator.free(v);
    if (result.org_id) |v| std.testing.allocator.free(v);
    if (result.workspace_id) |v| std.testing.allocator.free(v);
    if (result.audience) |v| std.testing.allocator.free(v);
    if (result.scopes) |v| std.testing.allocator.free(v);
}

test "extractClerkClaims from metadata.tenant_id + space-delimited scope claim" {
    const json =
        \\{"sub":"user_1","iss":"https://clerk.example.com","aud":"https://api.agentsfleet.net","scope":"fleet:read secret:write","exp":9999999999,"org_id":"org_1","metadata":{"tenant_id":"tenant_a","workspace_id":"ws_a"}}
    ;
    const result = try extractClerkClaims(std.testing.allocator, json);
    defer freeClaims(result);
    try std.testing.expectEqualStrings("tenant_a", result.tenant_id.?);
    try std.testing.expectEqualStrings("org_1", result.org_id.?);
    try std.testing.expectEqualStrings("ws_a", result.workspace_id.?);
    try std.testing.expectEqualStrings("https://api.agentsfleet.net", result.audience.?);
    try std.testing.expectEqualStrings("fleet:read secret:write", result.scopes.?);
}

test "extractClerkClaims from top-level tenant_id, no scopes" {
    const json =
        \\{"sub":"user_1","iss":"https://clerk.example.com","exp":9999999999,"tenant_id":"tenant_b","workspace_id":"ws_b"}
    ;
    const result = try extractClerkClaims(std.testing.allocator, json);
    defer freeClaims(result);
    try std.testing.expectEqualStrings("tenant_b", result.tenant_id.?);
    try std.testing.expectEqualStrings("ws_b", result.workspace_id.?);
    try std.testing.expect(result.org_id == null);
    try std.testing.expect(result.audience == null);
    try std.testing.expect(result.scopes == null);
}

test "extractClerkClaims with no tenant or org yields all-null" {
    const json =
        \\{"sub":"user_1","iss":"https://clerk.example.com","exp":9999999999}
    ;
    const result = try extractClerkClaims(std.testing.allocator, json);
    defer freeClaims(result);
    try std.testing.expect(result.tenant_id == null);
    try std.testing.expect(result.org_id == null);
    try std.testing.expect(result.workspace_id == null);
    try std.testing.expect(result.audience == null);
    try std.testing.expect(result.scopes == null);
}

test "extractClerkClaims reads the camel workspace key" {
    const json =
        \\{"sub":"user_6","iss":"https://clerk.example.com","exp":9999999999,"workspaceId":"ws_camel"}
    ;
    const result = try extractClerkClaims(std.testing.allocator, json);
    defer freeClaims(result);
    try std.testing.expectEqualStrings("ws_camel", result.workspace_id.?);
}

test "extractCustomClaims normalizes namespaced claims and aud array; scp array → space-joined" {
    const json =
        \\{"sub":"user_2","iss":"https://idp.example.com/","aud":["https://api.agentsfleet.net","https://userinfo.example.com"],"scp":["fleet:read","fleet:write"],"organization_id":"org_custom_ns","https://agentsfleet.net/tenant_id":"tenant_custom_ns","https://agentsfleet.net/workspace_id":"ws_custom_ns"}
    ;
    const result = try extractCustomClaims(std.testing.allocator, json);
    defer freeClaims(result);
    try std.testing.expectEqualStrings("tenant_custom_ns", result.tenant_id.?);
    try std.testing.expectEqualStrings("org_custom_ns", result.org_id.?);
    try std.testing.expectEqualStrings("ws_custom_ns", result.workspace_id.?);
    try std.testing.expectEqualStrings("https://api.agentsfleet.net", result.audience.?);
    try std.testing.expectEqualStrings("fleet:read fleet:write", result.scopes.?);
}

test "extractCustomClaims normalizes nested custom_claims payload + scopes array" {
    const json =
        \\{"sub":"user_3","iss":"https://idp.example.com","aud":"https://api.agentsfleet.net","custom_claims":{"tenant_id":"tenant_custom","workspaceId":"ws_custom","organization_id":"org_custom"},"scopes":["fleet:read","workspace:admin"]}
    ;
    const result = try extractCustomClaims(std.testing.allocator, json);
    defer freeClaims(result);
    try std.testing.expectEqualStrings("tenant_custom", result.tenant_id.?);
    try std.testing.expectEqualStrings("org_custom", result.org_id.?);
    try std.testing.expectEqualStrings("ws_custom", result.workspace_id.?);
    try std.testing.expectEqualStrings("https://api.agentsfleet.net", result.audience.?);
    try std.testing.expectEqualStrings("fleet:read workspace:admin", result.scopes.?);
}

test "extractCustomClaims joins only string scopes from mixed arrays" {
    const json =
        \\{"sub":"user_7","iss":"https://idp.example.com","scp":["fleet:read",3,"workspace:admin",true]}
    ;
    const result = try extractCustomClaims(std.testing.allocator, json);
    defer freeClaims(result);
    try std.testing.expectEqualStrings("fleet:read workspace:admin", result.scopes.?);
}

test "extractCustomClaims returns null scopes for empty scp array" {
    const json =
        \\{"sub":"user_12","iss":"https://idp.example.com","scp":[]}
    ;
    const result = try extractCustomClaims(std.testing.allocator, json);
    defer freeClaims(result);
    try std.testing.expect(result.scopes == null);
}

test "extractCustomClaims returns null scopes for non-string array elements" {
    const json =
        \\{"sub":"user_13","iss":"https://idp.example.com","scp":[1,2,false]}
    ;
    const result = try extractCustomClaims(std.testing.allocator, json);
    defer freeClaims(result);
    try std.testing.expect(result.scopes == null);
}

test "extractClerkClaims handles metadata that is not an object" {
    const json =
        \\{"sub":"user_11","iss":"https://clerk.example.com","exp":9999999999,"metadata":"not_an_object"}
    ;
    const result = try extractClerkClaims(std.testing.allocator, json);
    defer freeClaims(result);
    try std.testing.expect(result.tenant_id == null);
    try std.testing.expect(result.workspace_id == null);
}

test "extractClerkClaims rejects non-JSON / non-object / scalar JSON" {
    try std.testing.expectError(jwks.VerifyError.TokenMalformed, extractClerkClaims(std.testing.allocator, "not json"));
    try std.testing.expectError(jwks.VerifyError.TokenMalformed, extractClerkClaims(std.testing.allocator, "[1,2,3]"));
    try std.testing.expectError(jwks.VerifyError.TokenMalformed, extractClerkClaims(std.testing.allocator, ""));
    try std.testing.expectError(jwks.VerifyError.TokenMalformed, extractClerkClaims(std.testing.allocator, "42"));
    try std.testing.expectError(jwks.VerifyError.TokenMalformed, extractClerkClaims(std.testing.allocator, "true"));
    try std.testing.expectError(jwks.VerifyError.TokenMalformed, extractClerkClaims(std.testing.allocator, "null"));
    try std.testing.expectError(jwks.VerifyError.TokenMalformed, extractClerkClaims(std.testing.allocator, "\"just a string\""));
}

test "extractCustomClaims rejects malformed and non-object JSON" {
    try std.testing.expectError(jwks.VerifyError.TokenMalformed, extractCustomClaims(std.testing.allocator, ""));
    try std.testing.expectError(jwks.VerifyError.TokenMalformed, extractCustomClaims(std.testing.allocator, "not json"));
    try std.testing.expectError(jwks.VerifyError.TokenMalformed, extractCustomClaims(std.testing.allocator, "[1,2,3]"));
    try std.testing.expectError(jwks.VerifyError.TokenMalformed, extractCustomClaims(std.testing.allocator, "null"));
}
