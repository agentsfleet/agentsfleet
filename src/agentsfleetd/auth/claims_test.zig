const std = @import("std");
const jwks = @import("jwks.zig");
const claims = @import("claims.zig");

const IdentityClaims = claims.IdentityClaims;
const extractClerkClaims = claims.extractClerkClaims;
const extractCustomClaims = claims.extractCustomClaims;

test "extractClerkClaims from metadata.tenant_id" {
    const json =
        \\{"sub":"user_1","iss":"https://clerk.example.com","aud":"https://api.agentsfleet.net","scope":"runs:read runs:write","exp":9999999999,"org_id":"org_1","metadata":{"tenant_id":"tenant_a","workspace_id":"ws_a"}}
    ;
    const result = try extractClerkClaims(std.testing.allocator, json);
    defer {
        if (result.tenant_id) |v| std.testing.allocator.free(v);
        if (result.org_id) |v| std.testing.allocator.free(v);
        if (result.workspace_id) |v| std.testing.allocator.free(v);
        if (result.role) |v| std.testing.allocator.free(v);
        if (result.audience) |v| std.testing.allocator.free(v);
        if (result.scopes) |v| std.testing.allocator.free(v);
    }
    try std.testing.expectEqualStrings("tenant_a", result.tenant_id.?);
    try std.testing.expectEqualStrings("org_1", result.org_id.?);
    try std.testing.expectEqualStrings("ws_a", result.workspace_id.?);
    try std.testing.expect(result.role == null);
    try std.testing.expectEqualStrings("https://api.agentsfleet.net", result.audience.?);
    try std.testing.expectEqualStrings("runs:read runs:write", result.scopes.?);
}

test "extractClerkClaims from top-level tenant_id" {
    const json =
        \\{"sub":"user_1","iss":"https://clerk.example.com","exp":9999999999,"tenant_id":"tenant_b","workspace_id":"ws_b"}
    ;
    const result = try extractClerkClaims(std.testing.allocator, json);
    defer {
        if (result.tenant_id) |v| std.testing.allocator.free(v);
        if (result.org_id) |v| std.testing.allocator.free(v);
        if (result.workspace_id) |v| std.testing.allocator.free(v);
        if (result.role) |v| std.testing.allocator.free(v);
        if (result.audience) |v| std.testing.allocator.free(v);
        if (result.scopes) |v| std.testing.allocator.free(v);
    }
    try std.testing.expectEqualStrings("tenant_b", result.tenant_id.?);
    try std.testing.expectEqualStrings("ws_b", result.workspace_id.?);
    try std.testing.expect(result.org_id == null);
    try std.testing.expect(result.role == null);
    try std.testing.expect(result.audience == null);
    try std.testing.expect(result.scopes == null);
}

test "extractClerkClaims with no tenant or org" {
    const json =
        \\{"sub":"user_1","iss":"https://clerk.example.com","exp":9999999999}
    ;
    const result = try extractClerkClaims(std.testing.allocator, json);
    defer {
        if (result.tenant_id) |v| std.testing.allocator.free(v);
        if (result.org_id) |v| std.testing.allocator.free(v);
        if (result.workspace_id) |v| std.testing.allocator.free(v);
        if (result.role) |v| std.testing.allocator.free(v);
        if (result.audience) |v| std.testing.allocator.free(v);
        if (result.scopes) |v| std.testing.allocator.free(v);
    }
    try std.testing.expect(result.tenant_id == null);
    try std.testing.expect(result.org_id == null);
    try std.testing.expect(result.workspace_id == null);
    try std.testing.expect(result.role == null);
    try std.testing.expect(result.audience == null);
    try std.testing.expect(result.scopes == null);
}

test "extractCustomClaims normalizes namespaced claims and aud array" {
    const json =
        \\{"sub":"user_2","iss":"https://idp.example.com/","aud":["https://api.agentsfleet.net","https://userinfo.example.com"],"scp":["runs:read","runs:write"],"organization_id":"org_custom_ns","https://usezombie.dev/tenant_id":"tenant_custom_ns","https://usezombie.dev/workspace_id":"ws_custom_ns"}
    ;
    const result = try extractCustomClaims(std.testing.allocator, json);
    defer {
        if (result.tenant_id) |v| std.testing.allocator.free(v);
        if (result.org_id) |v| std.testing.allocator.free(v);
        if (result.workspace_id) |v| std.testing.allocator.free(v);
        if (result.role) |v| std.testing.allocator.free(v);
        if (result.audience) |v| std.testing.allocator.free(v);
        if (result.scopes) |v| std.testing.allocator.free(v);
    }
    try std.testing.expectEqualStrings("tenant_custom_ns", result.tenant_id.?);
    try std.testing.expectEqualStrings("org_custom_ns", result.org_id.?);
    try std.testing.expectEqualStrings("ws_custom_ns", result.workspace_id.?);
    try std.testing.expect(result.role == null);
    try std.testing.expectEqualStrings("https://api.agentsfleet.net", result.audience.?);
    try std.testing.expectEqualStrings("runs:read runs:write", result.scopes.?);
}

test "extractCustomClaims normalizes nested custom_claims payload" {
    const json =
        \\{"sub":"user_3","iss":"https://idp.example.com","aud":"https://api.agentsfleet.net","custom_claims":{"tenant_id":"tenant_custom","workspaceId":"ws_custom","organization_id":"org_custom"},"scopes":["runs:read","workspace:pause"]}
    ;
    const result = try extractCustomClaims(std.testing.allocator, json);
    defer {
        if (result.tenant_id) |v| std.testing.allocator.free(v);
        if (result.org_id) |v| std.testing.allocator.free(v);
        if (result.workspace_id) |v| std.testing.allocator.free(v);
        if (result.role) |v| std.testing.allocator.free(v);
        if (result.audience) |v| std.testing.allocator.free(v);
        if (result.scopes) |v| std.testing.allocator.free(v);
    }
    try std.testing.expectEqualStrings("tenant_custom", result.tenant_id.?);
    try std.testing.expectEqualStrings("org_custom", result.org_id.?);
    try std.testing.expectEqualStrings("ws_custom", result.workspace_id.?);
    try std.testing.expect(result.role == null);
    try std.testing.expectEqualStrings("https://api.agentsfleet.net", result.audience.?);
    try std.testing.expectEqualStrings("runs:read workspace:pause", result.scopes.?);
}

test "extractClerkClaims reads role from metadata" {
    const json =
        \\{"sub":"user_4","iss":"https://clerk.example.com","exp":9999999999,"metadata":{"tenant_id":"tenant_role","workspace_id":"ws_role","role":"operator"}}
    ;
    const result = try extractClerkClaims(std.testing.allocator, json);
    defer {
        if (result.tenant_id) |v| std.testing.allocator.free(v);
        if (result.org_id) |v| std.testing.allocator.free(v);
        if (result.workspace_id) |v| std.testing.allocator.free(v);
        if (result.role) |v| std.testing.allocator.free(v);
        if (result.audience) |v| std.testing.allocator.free(v);
        if (result.scopes) |v| std.testing.allocator.free(v);
    }
    try std.testing.expectEqualStrings("operator", result.role.?);
}

test "extractCustomClaims reads top-level or nested role claims" {
    const json =
        \\{"sub":"user_5","iss":"https://idp.example.com","custom_claims":{"tenant_id":"tenant_role","workspaceId":"ws_role","role":"admin"}}
    ;
    const result = try extractCustomClaims(std.testing.allocator, json);
    defer {
        if (result.tenant_id) |v| std.testing.allocator.free(v);
        if (result.org_id) |v| std.testing.allocator.free(v);
        if (result.workspace_id) |v| std.testing.allocator.free(v);
        if (result.role) |v| std.testing.allocator.free(v);
        if (result.audience) |v| std.testing.allocator.free(v);
        if (result.scopes) |v| std.testing.allocator.free(v);
    }
    try std.testing.expectEqualStrings("admin", result.role.?);
}

test "extractClerkClaims reads top-level role and camel workspace key" {
    const json =
        \\{"sub":"user_6","iss":"https://clerk.example.com","exp":9999999999,"role":"admin","workspaceId":"ws_camel"}
    ;
    const result = try extractClerkClaims(std.testing.allocator, json);
    defer {
        if (result.tenant_id) |v| std.testing.allocator.free(v);
        if (result.org_id) |v| std.testing.allocator.free(v);
        if (result.workspace_id) |v| std.testing.allocator.free(v);
        if (result.role) |v| std.testing.allocator.free(v);
        if (result.audience) |v| std.testing.allocator.free(v);
        if (result.scopes) |v| std.testing.allocator.free(v);
    }
    try std.testing.expectEqualStrings("admin", result.role.?);
    try std.testing.expectEqualStrings("ws_camel", result.workspace_id.?);
}

test "extractCustomClaims joins only string scopes from mixed arrays" {
    const json =
        \\{"sub":"user_7","iss":"https://idp.example.com","scp":["runs:read",3,"workspace:pause",true],"metadata":{"https://usezombie.dev/role":"operator"}}
    ;
    const result = try extractCustomClaims(std.testing.allocator, json);
    defer {
        if (result.tenant_id) |v| std.testing.allocator.free(v);
        if (result.org_id) |v| std.testing.allocator.free(v);
        if (result.workspace_id) |v| std.testing.allocator.free(v);
        if (result.role) |v| std.testing.allocator.free(v);
        if (result.audience) |v| std.testing.allocator.free(v);
        if (result.scopes) |v| std.testing.allocator.free(v);
    }
    try std.testing.expectEqualStrings("operator", result.role.?);
    try std.testing.expectEqualStrings("runs:read workspace:pause", result.scopes.?);
}

test "extractClerkClaims reads namespaced role claim (dev namespace)" {
    const json =
        \\{"sub":"user_ns1","iss":"https://clerk.example.com","exp":9999999999,"https://usezombie.dev/role":"operator","metadata":{"tenant_id":"tenant_ns"}}
    ;
    const result = try extractClerkClaims(std.testing.allocator, json);
    defer {
        if (result.tenant_id) |v| std.testing.allocator.free(v);
        if (result.org_id) |v| std.testing.allocator.free(v);
        if (result.workspace_id) |v| std.testing.allocator.free(v);
        if (result.role) |v| std.testing.allocator.free(v);
        if (result.audience) |v| std.testing.allocator.free(v);
        if (result.scopes) |v| std.testing.allocator.free(v);
    }
    try std.testing.expectEqualStrings("operator", result.role.?);
    try std.testing.expectEqualStrings("tenant_ns", result.tenant_id.?);
}

test "extractClerkClaims reads namespaced role claim (prod namespace)" {
    const json =
        \\{"sub":"user_ns2","iss":"https://clerk.example.com","exp":9999999999,"https://agentsfleet.net/role":"admin"}
    ;
    const result = try extractClerkClaims(std.testing.allocator, json);
    defer {
        if (result.tenant_id) |v| std.testing.allocator.free(v);
        if (result.org_id) |v| std.testing.allocator.free(v);
        if (result.workspace_id) |v| std.testing.allocator.free(v);
        if (result.role) |v| std.testing.allocator.free(v);
        if (result.audience) |v| std.testing.allocator.free(v);
        if (result.scopes) |v| std.testing.allocator.free(v);
    }
    try std.testing.expectEqualStrings("admin", result.role.?);
}

test "extractClerkClaims reads namespaced role from metadata object" {
    const json =
        \\{"sub":"user_ns3","iss":"https://clerk.example.com","exp":9999999999,"metadata":{"https://usezombie.dev/role":"admin"}}
    ;
    const result = try extractClerkClaims(std.testing.allocator, json);
    defer {
        if (result.tenant_id) |v| std.testing.allocator.free(v);
        if (result.org_id) |v| std.testing.allocator.free(v);
        if (result.workspace_id) |v| std.testing.allocator.free(v);
        if (result.role) |v| std.testing.allocator.free(v);
        if (result.audience) |v| std.testing.allocator.free(v);
        if (result.scopes) |v| std.testing.allocator.free(v);
    }
    try std.testing.expectEqualStrings("admin", result.role.?);
}

test "extractClerkClaims skips unsupported top-level role when metadata has supported fallback" {
    const json =
        \\{"sub":"user_8","iss":"https://clerk.example.com","exp":9999999999,"role":"member","metadata":{"role":"operator"}}
    ;
    const result = try extractClerkClaims(std.testing.allocator, json);
    defer {
        if (result.tenant_id) |v| std.testing.allocator.free(v);
        if (result.org_id) |v| std.testing.allocator.free(v);
        if (result.workspace_id) |v| std.testing.allocator.free(v);
        if (result.role) |v| std.testing.allocator.free(v);
        if (result.audience) |v| std.testing.allocator.free(v);
        if (result.scopes) |v| std.testing.allocator.free(v);
    }
    try std.testing.expectEqualStrings("operator", result.role.?);
}

test "extractCustomClaims skips unsupported namespaced role when nested fallback is supported" {
    const json =
        \\{"sub":"user_9","iss":"https://idp.example.com","https://usezombie.dev/role":"member","custom_claims":{"role":"admin"}}
    ;
    const result = try extractCustomClaims(std.testing.allocator, json);
    defer {
        if (result.tenant_id) |v| std.testing.allocator.free(v);
        if (result.org_id) |v| std.testing.allocator.free(v);
        if (result.workspace_id) |v| std.testing.allocator.free(v);
        if (result.role) |v| std.testing.allocator.free(v);
        if (result.audience) |v| std.testing.allocator.free(v);
        if (result.scopes) |v| std.testing.allocator.free(v);
    }
    try std.testing.expectEqualStrings("admin", result.role.?);
}

test "extractClerkClaims rejects non-JSON" {
    try std.testing.expectError(jwks.VerifyError.TokenMalformed, extractClerkClaims(std.testing.allocator, "not json"));
}

test "extractClerkClaims rejects non-object JSON" {
    try std.testing.expectError(jwks.VerifyError.TokenMalformed, extractClerkClaims(std.testing.allocator, "[1,2,3]"));
}

test "extractClerkClaims rejects empty string" {
    try std.testing.expectError(jwks.VerifyError.TokenMalformed, extractClerkClaims(std.testing.allocator, ""));
}

test "extractClerkClaims rejects scalar JSON values" {
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

test "extractClerkClaims returns null role when all levels have unsupported roles" {
    const json =
        \\{"sub":"user_10","iss":"https://clerk.example.com","exp":9999999999,"role":"member","metadata":{"role":"superuser"}}
    ;
    const result = try extractClerkClaims(std.testing.allocator, json);
    defer {
        if (result.tenant_id) |v| std.testing.allocator.free(v);
        if (result.org_id) |v| std.testing.allocator.free(v);
        if (result.workspace_id) |v| std.testing.allocator.free(v);
        if (result.role) |v| std.testing.allocator.free(v);
        if (result.audience) |v| std.testing.allocator.free(v);
        if (result.scopes) |v| std.testing.allocator.free(v);
    }
    try std.testing.expect(result.role == null);
}

test "extractClerkClaims ignores unsupported role with no fallback" {
    const json =
        \\{"sub":"user_10","iss":"https://clerk.example.com","exp":9999999999,"role":"member"}
    ;
    const result = try extractClerkClaims(std.testing.allocator, json);
    defer {
        if (result.tenant_id) |v| std.testing.allocator.free(v);
        if (result.org_id) |v| std.testing.allocator.free(v);
        if (result.workspace_id) |v| std.testing.allocator.free(v);
        if (result.role) |v| std.testing.allocator.free(v);
        if (result.audience) |v| std.testing.allocator.free(v);
        if (result.scopes) |v| std.testing.allocator.free(v);
    }
    try std.testing.expect(result.role == null);
}

test "extractClerkClaims handles metadata that is not an object" {
    const json =
        \\{"sub":"user_11","iss":"https://clerk.example.com","exp":9999999999,"metadata":"not_an_object"}
    ;
    const result = try extractClerkClaims(std.testing.allocator, json);
    defer {
        if (result.tenant_id) |v| std.testing.allocator.free(v);
        if (result.org_id) |v| std.testing.allocator.free(v);
        if (result.workspace_id) |v| std.testing.allocator.free(v);
        if (result.role) |v| std.testing.allocator.free(v);
        if (result.audience) |v| std.testing.allocator.free(v);
        if (result.scopes) |v| std.testing.allocator.free(v);
    }
    try std.testing.expect(result.tenant_id == null);
    try std.testing.expect(result.workspace_id == null);
    try std.testing.expect(result.role == null);
}

test "extractCustomClaims returns null scopes for empty scp array" {
    const json =
        \\{"sub":"user_12","iss":"https://idp.example.com","scp":[]}
    ;
    const result = try extractCustomClaims(std.testing.allocator, json);
    defer {
        if (result.tenant_id) |v| std.testing.allocator.free(v);
        if (result.org_id) |v| std.testing.allocator.free(v);
        if (result.workspace_id) |v| std.testing.allocator.free(v);
        if (result.role) |v| std.testing.allocator.free(v);
        if (result.audience) |v| std.testing.allocator.free(v);
        if (result.scopes) |v| std.testing.allocator.free(v);
    }
    try std.testing.expect(result.scopes == null);
}

test "extractCustomClaims returns null scopes for non-string array elements" {
    const json =
        \\{"sub":"user_13","iss":"https://idp.example.com","scp":[1,2,false]}
    ;
    const result = try extractCustomClaims(std.testing.allocator, json);
    defer {
        if (result.tenant_id) |v| std.testing.allocator.free(v);
        if (result.org_id) |v| std.testing.allocator.free(v);
        if (result.workspace_id) |v| std.testing.allocator.free(v);
        if (result.role) |v| std.testing.allocator.free(v);
        if (result.audience) |v| std.testing.allocator.free(v);
        if (result.scopes) |v| std.testing.allocator.free(v);
    }
    try std.testing.expect(result.scopes == null);
}

fn freeClaims(result: IdentityClaims) void {
    if (result.tenant_id) |v| std.testing.allocator.free(v);
    if (result.org_id) |v| std.testing.allocator.free(v);
    if (result.workspace_id) |v| std.testing.allocator.free(v);
    if (result.role) |v| std.testing.allocator.free(v);
    if (result.audience) |v| std.testing.allocator.free(v);
    if (result.scopes) |v| std.testing.allocator.free(v);
}

test "extractClerkClaims parses platform_admin, fail-closed on absent/non-bool" {
    const cases = [_]struct { json: []const u8, want: bool }{
        // nested under metadata (the documented placement)
        .{ .json = "{\"sub\":\"u\",\"iss\":\"i\",\"metadata\":{\"platform_admin\":true}}", .want = true },
        // top-level boolean is honored too
        .{ .json = "{\"sub\":\"u\",\"iss\":\"i\",\"platform_admin\":true}", .want = true },
        // explicit false ⇒ false
        .{ .json = "{\"sub\":\"u\",\"iss\":\"i\",\"metadata\":{\"platform_admin\":false}}", .want = false },
        // absent ⇒ false (fail-closed)
        .{ .json = "{\"sub\":\"u\",\"iss\":\"i\",\"metadata\":{\"tenant_id\":\"t\"}}", .want = false },
        // a string "true" is NOT coerced ⇒ false
        .{ .json = "{\"sub\":\"u\",\"iss\":\"i\",\"platform_admin\":\"true\"}", .want = false },
    };
    for (cases) |c| {
        const result = try extractClerkClaims(std.testing.allocator, c.json);
        defer freeClaims(result);
        try std.testing.expectEqual(c.want, result.platform_admin);
    }
}

test "extractCustomClaims parses platform_admin from custom_claims, fail-closed when absent" {
    const present = try extractCustomClaims(
        std.testing.allocator,
        "{\"sub\":\"u\",\"iss\":\"i\",\"custom_claims\":{\"platform_admin\":true}}",
    );
    defer freeClaims(present);
    try std.testing.expect(present.platform_admin);

    const absent = try extractCustomClaims(
        std.testing.allocator,
        "{\"sub\":\"u\",\"iss\":\"i\",\"custom_claims\":{\"tenant_id\":\"t\"}}",
    );
    defer freeClaims(absent);
    try std.testing.expect(!absent.platform_admin);
}
