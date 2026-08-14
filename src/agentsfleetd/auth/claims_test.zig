//! Tests for verified-JWT claim extraction (role/platform_admin
//! claims removed; capability rides the `scopes` claim). FLL-exempt.

const std = @import("std");
const jwks = @import("jwks.zig");
const claims = @import("claims.zig");

const IdentityClaims = claims.IdentityClaims;
const extractClerkClaims = claims.extractClerkClaims;

fn freeClaims(result: IdentityClaims) void {
    if (result.tenant_id) |v| std.testing.allocator.free(v);
    if (result.org_id) |v| std.testing.allocator.free(v);
    if (result.workspace_id) |v| std.testing.allocator.free(v);
    if (result.audience) |v| std.testing.allocator.free(v);
    if (result.scopes) |v| std.testing.allocator.free(v);
}

test "extractClerkClaims from metadata.tenant_id + space-delimited scopes claim" {
    // The canonical shape: `metadata.tenant_id` nested, `scopes` at top level,
    // exactly as the session-token claim customization projects it
    // (docs/AUTH.md §Clerk org config). This fixture used to spell the claim
    // `scope`, the OAuth2 key nothing here writes, and passed anyway on the
    // since-removed fallback ladder — the suite's own happy path was proving a
    // shape production never sends.
    const json =
        \\{"sub":"user_1","iss":"https://clerk.example.com","aud":"https://api.agentsfleet.net","scopes":"fleet:read secret:write","exp":9999999999,"org_id":"org_1","metadata":{"tenant_id":"tenant_a","workspace_id":"ws_a"}}
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

test "claim materialization survives allocation failure without leaking" {
    // checkAllAllocationFailures fails each internal allocation in turn and
    // asserts the error return leaks nothing — the deterministic proof that
    // duplicateClaims' errdefer ladder frees every earlier dupe (and the
    // scopes slice) when a later dupe fails.
    const json =
        \\{"sub":"user_1","tenant_id":"tenant_a","org_id":"org_1","workspace_id":"ws_a","aud":"https://api.agentsfleet.net","scopes":"fleet:read secret:write"}
    ;
    const Probe = struct {
        fn freeAll(alloc: std.mem.Allocator, c: IdentityClaims) void {
            if (c.tenant_id) |v| alloc.free(v);
            if (c.org_id) |v| alloc.free(v);
            if (c.workspace_id) |v| alloc.free(v);
            if (c.audience) |v| alloc.free(v);
            if (c.scopes) |v| alloc.free(v);
        }
        fn run(alloc: std.mem.Allocator, payload: []const u8) !void {
            const clerk = try extractClerkClaims(alloc, payload);
            freeAll(alloc, clerk);
        }
    };
    try std.testing.checkAllAllocationFailures(std.testing.allocator, Probe.run, .{json});
}

// ── The capability claim is read from one key ───────────────────────────────

test "test_oauth2_scope_cannot_displace_provisioned_scopes — one key carries capability" {
    // The regression this file exists to hold. The reader used to try `scope`
    // BEFORE `scopes`, so a token carrying both resolved to the one we neither
    // write nor provision — silently swapping the caller's capability set on
    // the authorisation path. `scopes` is what the session-token template
    // projects and what renderMetadataPayload writes, so `scopes` is what wins.
    const json =
        \\{"sub":"user_20","iss":"https://clerk.example.com","scope":"workspace:any platform-key:admin","scopes":"fleet:read"}
    ;
    const result = try extractClerkClaims(std.testing.allocator, json);
    defer freeClaims(result);
    try std.testing.expectEqualStrings("fleet:read", result.scopes.?);
}

test "test_retired_scope_spellings_are_unread — no capability beats the wrong capability" {
    // `scope` (OAuth2) and `scp` (Azure/Auth0) are other providers' keys.
    // Nothing writes them here, so a token carrying only one carries no
    // capability — every gate then refuses it, which is the fail-closed
    // direction. Read as a fallback they would have been the fail-open one.
    const cases = [_][]const u8{
        \\{"sub":"user_21","iss":"https://clerk.example.com","scope":"workspace:any"}
        ,
        \\{"sub":"user_22","iss":"https://clerk.example.com","scp":"workspace:any"}
        ,
        \\{"sub":"user_23","iss":"https://clerk.example.com","scp":["workspace:any","fleet:admin"]}
        ,
    };
    for (cases) |json| {
        const result = try extractClerkClaims(std.testing.allocator, json);
        defer freeClaims(result);
        try std.testing.expect(result.scopes == null);
    }
}

test "test_retired_workspace_spelling_is_unread — an authz input needs a writer" {
    // Nothing ever wrote `workspace_id` into the provider's metadata in any
    // spelling — renderMetadataPayload writes `tenant_id` and `scopes` only —
    // so the camelCase alias had no writer. It mattered because a workspace id
    // NARROWS authorisation (common_authz.zig), and a narrowing input read
    // from a key nobody controls is one an attacker gets to choose.
    const json =
        \\{"sub":"user_24","iss":"https://clerk.example.com","workspaceId":"ws_camel"}
    ;
    const result = try extractClerkClaims(std.testing.allocator, json);
    defer freeClaims(result);
    try std.testing.expect(result.workspace_id == null);
}
