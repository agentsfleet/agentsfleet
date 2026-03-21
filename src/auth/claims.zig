//! Provider-specific claim normalization from verified JWT payloads.

const std = @import("std");
const jwks = @import("jwks.zig");

const log = std.log.scoped(.auth);

pub const IdentityClaims = struct {
    tenant_id: ?[]u8,
    org_id: ?[]u8,
    workspace_id: ?[]u8,
    audience: ?[]u8,
    scopes: ?[]u8,
};

pub const ClerkClaims = IdentityClaims;
pub const CustomClaims = IdentityClaims;

const CLAIM_TENANT_ID = "tenant_id";
const CLAIM_ORG_ID = "org_id";
const CLAIM_ORGANIZATION_ID = "organization_id";
const CLAIM_WORKSPACE_ID = "workspace_id";
const CLAIM_WORKSPACE_CAMEL = "workspaceId";
const CLAIM_SCOPE = "scope";
const CLAIM_SCOPES = "scopes";
const CLAIM_SCP = "scp";
const CLAIM_AUD = "aud";
const NAMESPACE_DEV = "https://usezombie.dev/";
const NAMESPACE_PROD = "https://usezombie.com/";

/// Extract Clerk-specific claims from a verified JWT payload.
/// Looks for `org_id` at top level and `tenant_id`/`workspace_id`
/// at top level or nested under `metadata`.
pub fn extractClerkClaims(alloc: std.mem.Allocator, claims_json: []const u8) !ClerkClaims {
    const parsed = try parseClaimsObject(alloc, claims_json);
    defer parsed.deinit();

    const tenant_id = getClerkTenantId(parsed.value.object);
    const org_id = getClerkOrgId(parsed.value.object);
    log.debug("clerk claims tenant_id={s} org_id={s}", .{
        if (tenant_id) |v| v else "missing",
        if (org_id) |v| v else "missing",
    });

    return duplicateClaims(alloc, .{
        .tenant_id = tenant_id,
        .org_id = org_id,
        .workspace_id = getClerkWorkspaceId(parsed.value.object),
        .audience = getAudience(parsed.value.object),
        .scopes = try getScopesOwned(alloc, parsed.value.object),
    });
}

/// Extract claims from a custom OIDC provider. This path accepts the common
/// top-level form plus nested `metadata`, `app_metadata`, or `custom_claims`.
pub fn extractCustomClaims(alloc: std.mem.Allocator, claims_json: []const u8) !CustomClaims {
    const parsed = try parseClaimsObject(alloc, claims_json);
    defer parsed.deinit();

    const tenant_id = getCustomTenantId(parsed.value.object);
    const org_id = getCustomOrgId(parsed.value.object);
    log.debug("custom claims tenant_id={s} org_id={s}", .{
        if (tenant_id) |v| v else "missing",
        if (org_id) |v| v else "missing",
    });

    return duplicateClaims(alloc, .{
        .tenant_id = tenant_id,
        .org_id = org_id,
        .workspace_id = getCustomWorkspaceId(parsed.value.object),
        .audience = getAudience(parsed.value.object),
        .scopes = try getScopesOwned(alloc, parsed.value.object),
    });
}

fn parseClaimsObject(alloc: std.mem.Allocator, claims_json: []const u8) !std.json.Parsed(std.json.Value) {
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, claims_json, .{}) catch
        return jwks.VerifyError.TokenMalformed;
    if (parsed.value != .object) {
        parsed.deinit();
        return jwks.VerifyError.TokenMalformed;
    }
    return parsed;
}

fn duplicateClaims(alloc: std.mem.Allocator, view: struct {
    tenant_id: ?[]const u8,
    org_id: ?[]const u8,
    workspace_id: ?[]const u8,
    audience: ?[]const u8,
    scopes: ?[]u8,
}) !IdentityClaims {
    errdefer if (view.scopes) |v| alloc.free(v);

    return .{
        .tenant_id = if (view.tenant_id) |v| try alloc.dupe(u8, v) else null,
        .org_id = if (view.org_id) |v| try alloc.dupe(u8, v) else null,
        .workspace_id = if (view.workspace_id) |v| try alloc.dupe(u8, v) else null,
        .audience = if (view.audience) |v| try alloc.dupe(u8, v) else null,
        .scopes = view.scopes,
    };
}

fn getClerkTenantId(obj: std.json.ObjectMap) ?[]const u8 {
    if (jwks.getString(obj, CLAIM_TENANT_ID)) |v| return v;

    const metadata = obj.get("metadata") orelse return null;
    if (metadata != .object) return null;
    return jwks.getString(metadata.object, CLAIM_TENANT_ID);
}

fn getClerkOrgId(obj: std.json.ObjectMap) ?[]const u8 {
    return jwks.getString(obj, CLAIM_ORG_ID);
}

fn getClerkWorkspaceId(obj: std.json.ObjectMap) ?[]const u8 {
    if (jwks.getString(obj, CLAIM_WORKSPACE_ID)) |v| return v;
    if (jwks.getString(obj, CLAIM_WORKSPACE_CAMEL)) |v| return v;

    const metadata = obj.get("metadata") orelse return null;
    if (metadata != .object) return null;
    if (jwks.getString(metadata.object, CLAIM_WORKSPACE_ID)) |v| return v;
    return jwks.getString(metadata.object, CLAIM_WORKSPACE_CAMEL);
}

fn getCustomTenantId(obj: std.json.ObjectMap) ?[]const u8 {
    return getFirstValue(obj, &.{
        CLAIM_TENANT_ID,
        NAMESPACE_DEV ++ CLAIM_TENANT_ID,
        NAMESPACE_PROD ++ CLAIM_TENANT_ID,
    }, &.{ "custom_claims", "metadata", "app_metadata" });
}

fn getCustomOrgId(obj: std.json.ObjectMap) ?[]const u8 {
    return getFirstValue(obj, &.{
        CLAIM_ORG_ID,
        CLAIM_ORGANIZATION_ID,
        NAMESPACE_DEV ++ CLAIM_ORGANIZATION_ID,
        NAMESPACE_PROD ++ CLAIM_ORGANIZATION_ID,
    }, &.{ "custom_claims", "metadata", "app_metadata" });
}

fn getCustomWorkspaceId(obj: std.json.ObjectMap) ?[]const u8 {
    return getFirstValue(obj, &.{
        CLAIM_WORKSPACE_ID,
        CLAIM_WORKSPACE_CAMEL,
        NAMESPACE_DEV ++ CLAIM_WORKSPACE_ID,
        NAMESPACE_DEV ++ CLAIM_WORKSPACE_CAMEL,
        NAMESPACE_PROD ++ CLAIM_WORKSPACE_ID,
        NAMESPACE_PROD ++ CLAIM_WORKSPACE_CAMEL,
    }, &.{ "custom_claims", "metadata", "app_metadata" });
}

fn getFirstValue(
    obj: std.json.ObjectMap,
    direct_keys: []const []const u8,
    nested_objects: []const []const u8,
) ?[]const u8 {
    for (direct_keys) |key| {
        if (jwks.getString(obj, key)) |v| return v;
    }
    for (nested_objects) |nested| {
        const child = obj.get(nested) orelse continue;
        if (child != .object) continue;
        for (direct_keys) |key| {
            if (jwks.getString(child.object, key)) |v| return v;
        }
    }
    return null;
}

fn getAudience(obj: std.json.ObjectMap) ?[]const u8 {
    const aud = obj.get(CLAIM_AUD) orelse return null;
    return switch (aud) {
        .string => aud.string,
        .array => for (aud.array.items) |item| {
            if (item == .string) break item.string;
        } else null,
        else => null,
    };
}

fn getScopesOwned(alloc: std.mem.Allocator, obj: std.json.ObjectMap) !?[]u8 {
    if (jwks.getString(obj, CLAIM_SCOPE)) |v| return try alloc.dupe(u8, v);
    if (jwks.getString(obj, CLAIM_SCOPES)) |v| return try alloc.dupe(u8, v);
    if (jwks.getString(obj, CLAIM_SCP)) |v| return try alloc.dupe(u8, v);

    const scp = obj.get(CLAIM_SCP) orelse obj.get(CLAIM_SCOPES) orelse return null;
    if (scp != .array) return null;

    var out = std.ArrayList(u8){};
    defer out.deinit(alloc);
    for (scp.array.items, 0..) |item, idx| {
        if (item != .string) continue;
        if (idx != 0 and out.items.len > 0) try out.append(alloc, ' ');
        try out.appendSlice(alloc, item.string);
    }
    if (out.items.len == 0) return null;
    return try out.toOwnedSlice(alloc);
}

test "extractClerkClaims from metadata.tenant_id" {
    const json =
        \\{"sub":"user_1","iss":"https://clerk.example.com","aud":"https://api.usezombie.com","scope":"runs:read runs:write","exp":9999999999,"org_id":"org_1","metadata":{"tenant_id":"tenant_a","workspace_id":"ws_a"}}
    ;
    const result = try extractClerkClaims(std.testing.allocator, json);
    defer {
        if (result.tenant_id) |v| std.testing.allocator.free(v);
        if (result.org_id) |v| std.testing.allocator.free(v);
        if (result.workspace_id) |v| std.testing.allocator.free(v);
        if (result.audience) |v| std.testing.allocator.free(v);
        if (result.scopes) |v| std.testing.allocator.free(v);
    }
    try std.testing.expectEqualStrings("tenant_a", result.tenant_id.?);
    try std.testing.expectEqualStrings("org_1", result.org_id.?);
    try std.testing.expectEqualStrings("ws_a", result.workspace_id.?);
    try std.testing.expectEqualStrings("https://api.usezombie.com", result.audience.?);
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
        if (result.audience) |v| std.testing.allocator.free(v);
        if (result.scopes) |v| std.testing.allocator.free(v);
    }
    try std.testing.expectEqualStrings("tenant_b", result.tenant_id.?);
    try std.testing.expectEqualStrings("ws_b", result.workspace_id.?);
    try std.testing.expect(result.org_id == null);
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
        if (result.audience) |v| std.testing.allocator.free(v);
        if (result.scopes) |v| std.testing.allocator.free(v);
    }
    try std.testing.expect(result.tenant_id == null);
    try std.testing.expect(result.org_id == null);
    try std.testing.expect(result.workspace_id == null);
    try std.testing.expect(result.audience == null);
    try std.testing.expect(result.scopes == null);
}

test "extractCustomClaims normalizes namespaced claims and aud array" {
    const json =
        \\{"sub":"user_2","iss":"https://idp.example.com/","aud":["https://api.usezombie.com","https://userinfo.example.com"],"scp":["runs:read","runs:write"],"organization_id":"org_custom_ns","https://usezombie.dev/tenant_id":"tenant_custom_ns","https://usezombie.dev/workspace_id":"ws_custom_ns"}
    ;
    const result = try extractCustomClaims(std.testing.allocator, json);
    defer {
        if (result.tenant_id) |v| std.testing.allocator.free(v);
        if (result.org_id) |v| std.testing.allocator.free(v);
        if (result.workspace_id) |v| std.testing.allocator.free(v);
        if (result.audience) |v| std.testing.allocator.free(v);
        if (result.scopes) |v| std.testing.allocator.free(v);
    }
    try std.testing.expectEqualStrings("tenant_custom_ns", result.tenant_id.?);
    try std.testing.expectEqualStrings("org_custom_ns", result.org_id.?);
    try std.testing.expectEqualStrings("ws_custom_ns", result.workspace_id.?);
    try std.testing.expectEqualStrings("https://api.usezombie.com", result.audience.?);
    try std.testing.expectEqualStrings("runs:read runs:write", result.scopes.?);
}

test "extractCustomClaims normalizes nested custom_claims payload" {
    const json =
        \\{"sub":"user_3","iss":"https://idp.example.com","aud":"https://api.usezombie.com","custom_claims":{"tenant_id":"tenant_custom","workspaceId":"ws_custom","organization_id":"org_custom"},"scopes":["runs:read","workspace:pause"]}
    ;
    const result = try extractCustomClaims(std.testing.allocator, json);
    defer {
        if (result.tenant_id) |v| std.testing.allocator.free(v);
        if (result.org_id) |v| std.testing.allocator.free(v);
        if (result.workspace_id) |v| std.testing.allocator.free(v);
        if (result.audience) |v| std.testing.allocator.free(v);
        if (result.scopes) |v| std.testing.allocator.free(v);
    }
    try std.testing.expectEqualStrings("tenant_custom", result.tenant_id.?);
    try std.testing.expectEqualStrings("org_custom", result.org_id.?);
    try std.testing.expectEqualStrings("ws_custom", result.workspace_id.?);
    try std.testing.expectEqualStrings("https://api.usezombie.com", result.audience.?);
    try std.testing.expectEqualStrings("runs:read workspace:pause", result.scopes.?);
}

test "extractClerkClaims rejects non-JSON" {
    try std.testing.expectError(jwks.VerifyError.TokenMalformed, extractClerkClaims(std.testing.allocator, "not json"));
}

test "extractClerkClaims rejects non-object JSON" {
    try std.testing.expectError(jwks.VerifyError.TokenMalformed, extractClerkClaims(std.testing.allocator, "[1,2,3]"));
}
