//! Provider-specific claim normalization from verified JWT payloads.

const std = @import("std");
const jwks = @import("jwks.zig");
const rbac = @import("rbac.zig");
const logging = @import("log");

const log = logging.scoped(.auth);

const S_CUSTOM_CLAIMS = "custom_claims";
const S_METADATA = "metadata";
const S_APP_METADATA = "app_metadata";
const S_MISSING = "missing";

pub const IdentityClaims = struct {
    tenant_id: ?[]u8,
    org_id: ?[]u8,
    workspace_id: ?[]u8,
    role: ?[]u8,
    audience: ?[]u8,
    scopes: ?[]u8,
    /// usezombie platform-operator flag. Read from the `platform_admin` boolean
    /// claim (top-level or nested under metadata/custom_claims). Fail-closed:
    /// absent or non-bool ⇒ false. A bool, so it carries no allocation.
    platform_admin: bool,
};

const ClerkClaims = IdentityClaims;
const CustomClaims = IdentityClaims;

const CLAIM_TENANT_ID = "tenant_id";
const CLAIM_ORG_ID = "org_id";
const CLAIM_ORGANIZATION_ID = "organization_id";
const CLAIM_WORKSPACE_ID = "workspace_id";
const CLAIM_WORKSPACE_CAMEL = "workspaceId";
const CLAIM_SCOPE = "scope";
const CLAIM_SCOPES = "scopes";
const CLAIM_SCP = "scp";
const CLAIM_AUD = "aud";
const CLAIM_ROLE = "role";
const CLAIM_PLATFORM_ADMIN = "platform_admin";

// JWT claim namespace prefixes — these must match the identity provider's
// custom claim configuration (Clerk/Auth0). Not user-configurable.
const NAMESPACE_DEV = "https://usezombie.dev/";
const NAMESPACE_PROD = "https://usezombie.com/";

/// Validate and normalize a role string using the canonical RBAC enum.
/// Returns the canonical label or null if the role is not recognized.
fn normalizeSupportedRole(raw: []const u8) ?[]const u8 {
    const role = rbac.parseAuthRole(raw) orelse return null;
    return role.label();
}

/// Extract Clerk-specific claims from a verified JWT payload.
/// Looks for `org_id` at top level and `tenant_id`/`workspace_id`
/// at top level or nested under `metadata`.
pub fn extractClerkClaims(alloc: std.mem.Allocator, claims_json: []const u8) !ClerkClaims {
    const parsed = try parseClaimsObject(alloc, claims_json);
    defer parsed.deinit();

    const tenant_id = getClerkTenantId(parsed.value.object);
    const org_id = getClerkOrgId(parsed.value.object);
    log.debug("clerk_claims_extracted", .{
        .tenant_id = if (tenant_id) |v| v else S_MISSING,
        .org_id = if (org_id) |v| v else S_MISSING,
    });

    return duplicateClaims(alloc, .{
        .tenant_id = tenant_id,
        .org_id = org_id,
        .workspace_id = getClerkWorkspaceId(parsed.value.object),
        .role = getClerkRole(parsed.value.object),
        .audience = getAudience(parsed.value.object),
        .scopes = try getScopesOwned(alloc, parsed.value.object),
        .platform_admin = getClerkPlatformAdmin(parsed.value.object),
    });
}

/// Extract claims from a custom OIDC provider. This path accepts the common
/// top-level form plus nested `metadata`, `app_metadata`, or `custom_claims`.
pub fn extractCustomClaims(alloc: std.mem.Allocator, claims_json: []const u8) !CustomClaims {
    const parsed = try parseClaimsObject(alloc, claims_json);
    defer parsed.deinit();

    const tenant_id = getCustomTenantId(parsed.value.object);
    const org_id = getCustomOrgId(parsed.value.object);
    log.debug("custom_claims_extracted", .{
        .tenant_id = if (tenant_id) |v| v else S_MISSING,
        .org_id = if (org_id) |v| v else S_MISSING,
    });

    return duplicateClaims(alloc, .{
        .tenant_id = tenant_id,
        .org_id = org_id,
        .workspace_id = getCustomWorkspaceId(parsed.value.object),
        .role = getCustomRole(parsed.value.object),
        .audience = getAudience(parsed.value.object),
        .scopes = try getScopesOwned(alloc, parsed.value.object),
        .platform_admin = getCustomPlatformAdmin(parsed.value.object),
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
    role: ?[]const u8,
    audience: ?[]const u8,
    scopes: ?[]u8,
    platform_admin: bool,
}) !IdentityClaims {
    errdefer if (view.scopes) |v| alloc.free(v);

    return .{
        .tenant_id = if (view.tenant_id) |v| try alloc.dupe(u8, v) else null,
        .org_id = if (view.org_id) |v| try alloc.dupe(u8, v) else null,
        .workspace_id = if (view.workspace_id) |v| try alloc.dupe(u8, v) else null,
        .role = if (view.role) |v| try alloc.dupe(u8, v) else null,
        .audience = if (view.audience) |v| try alloc.dupe(u8, v) else null,
        .scopes = view.scopes,
        .platform_admin = view.platform_admin,
    };
}

fn getClerkTenantId(obj: std.json.ObjectMap) ?[]const u8 {
    if (jwks.getString(obj, CLAIM_TENANT_ID)) |v| return v;

    const metadata = obj.get(S_METADATA) orelse return null;
    if (metadata != .object) return null;
    return jwks.getString(metadata.object, CLAIM_TENANT_ID);
}

fn getClerkOrgId(obj: std.json.ObjectMap) ?[]const u8 {
    return jwks.getString(obj, CLAIM_ORG_ID);
}

fn getClerkWorkspaceId(obj: std.json.ObjectMap) ?[]const u8 {
    if (jwks.getString(obj, CLAIM_WORKSPACE_ID)) |v| return v;
    if (jwks.getString(obj, CLAIM_WORKSPACE_CAMEL)) |v| return v;

    const metadata = obj.get(S_METADATA) orelse return null;
    if (metadata != .object) return null;
    if (jwks.getString(metadata.object, CLAIM_WORKSPACE_ID)) |v| return v;
    return jwks.getString(metadata.object, CLAIM_WORKSPACE_CAMEL);
}

fn getClerkRole(obj: std.json.ObjectMap) ?[]const u8 {
    return getFirstSupportedRole(obj, &.{
        CLAIM_ROLE,
        NAMESPACE_DEV ++ CLAIM_ROLE,
        NAMESPACE_PROD ++ CLAIM_ROLE,
    }, &.{S_METADATA});
}

/// Read the `platform_admin` boolean claim, top-level or nested under metadata.
/// Fail-closed: any absent, non-object, or non-bool value resolves to `false`.
fn getClerkPlatformAdmin(obj: std.json.ObjectMap) bool {
    return getFirstBool(obj, CLAIM_PLATFORM_ADMIN, &.{S_METADATA});
}

/// Custom-OIDC variant: the deployment's own identity provider is the trust
/// root, so honor the bool from the common nest sites. Still fail-closed.
fn getCustomPlatformAdmin(obj: std.json.ObjectMap) bool {
    return getFirstBool(obj, CLAIM_PLATFORM_ADMIN, &.{ S_CUSTOM_CLAIMS, S_METADATA, S_APP_METADATA });
}

/// Look up a boolean claim at the top level, then in each nested object.
/// Returns `false` unless a real JSON `true` is found (never coerces strings).
fn getFirstBool(obj: std.json.ObjectMap, key: []const u8, nested_objects: []const []const u8) bool {
    if (boolValue(obj, key)) |v| return v;
    for (nested_objects) |nested| {
        const child = obj.get(nested) orelse continue;
        if (child != .object) continue;
        if (boolValue(child.object, key)) |v| return v;
    }
    return false;
}

fn boolValue(obj: std.json.ObjectMap, key: []const u8) ?bool {
    const v = obj.get(key) orelse return null;
    return switch (v) {
        .bool => |b| b,
        else => null,
    };
}

fn getCustomTenantId(obj: std.json.ObjectMap) ?[]const u8 {
    return getFirstValue(obj, &.{
        CLAIM_TENANT_ID,
        NAMESPACE_DEV ++ CLAIM_TENANT_ID,
        NAMESPACE_PROD ++ CLAIM_TENANT_ID,
    }, &.{ S_CUSTOM_CLAIMS, S_METADATA, S_APP_METADATA });
}

fn getCustomOrgId(obj: std.json.ObjectMap) ?[]const u8 {
    return getFirstValue(obj, &.{
        CLAIM_ORG_ID,
        CLAIM_ORGANIZATION_ID,
        NAMESPACE_DEV ++ CLAIM_ORGANIZATION_ID,
        NAMESPACE_PROD ++ CLAIM_ORGANIZATION_ID,
    }, &.{ S_CUSTOM_CLAIMS, S_METADATA, S_APP_METADATA });
}

fn getCustomWorkspaceId(obj: std.json.ObjectMap) ?[]const u8 {
    return getFirstValue(obj, &.{
        CLAIM_WORKSPACE_ID,
        CLAIM_WORKSPACE_CAMEL,
        NAMESPACE_DEV ++ CLAIM_WORKSPACE_ID,
        NAMESPACE_DEV ++ CLAIM_WORKSPACE_CAMEL,
        NAMESPACE_PROD ++ CLAIM_WORKSPACE_ID,
        NAMESPACE_PROD ++ CLAIM_WORKSPACE_CAMEL,
    }, &.{ S_CUSTOM_CLAIMS, S_METADATA, S_APP_METADATA });
}

fn getCustomRole(obj: std.json.ObjectMap) ?[]const u8 {
    return getFirstSupportedRole(obj, &.{
        CLAIM_ROLE,
        NAMESPACE_DEV ++ CLAIM_ROLE,
        NAMESPACE_PROD ++ CLAIM_ROLE,
    }, &.{ S_CUSTOM_CLAIMS, S_METADATA, S_APP_METADATA });
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

fn getFirstSupportedRole(
    obj: std.json.ObjectMap,
    direct_keys: []const []const u8,
    nested_objects: []const []const u8,
) ?[]const u8 {
    for (direct_keys) |key| {
        if (jwks.getString(obj, key)) |v| {
            if (normalizeSupportedRole(v)) |role| return role;
        }
    }
    for (nested_objects) |nested| {
        const child = obj.get(nested) orelse continue;
        if (child != .object) continue;
        for (direct_keys) |key| {
            if (jwks.getString(child.object, key)) |v| {
                if (normalizeSupportedRole(v)) |role| return role;
            }
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

    var buf: std.ArrayListUnmanaged(u8) = .empty;
    errdefer buf.deinit(alloc);
    for (scp.array.items) |item| {
        if (item != .string or item.string.len == 0) continue;
        if (buf.items.len > 0) try buf.append(alloc, ' ');
        try buf.appendSlice(alloc, item.string);
    }
    if (buf.items.len == 0) {
        buf.deinit(alloc);
        return null;
    }
    return try buf.toOwnedSlice(alloc);
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
        if (result.role) |v| std.testing.allocator.free(v);
        if (result.audience) |v| std.testing.allocator.free(v);
        if (result.scopes) |v| std.testing.allocator.free(v);
    }
    try std.testing.expectEqualStrings("tenant_a", result.tenant_id.?);
    try std.testing.expectEqualStrings("org_1", result.org_id.?);
    try std.testing.expectEqualStrings("ws_a", result.workspace_id.?);
    try std.testing.expect(result.role == null);
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
        \\{"sub":"user_2","iss":"https://idp.example.com/","aud":["https://api.usezombie.com","https://userinfo.example.com"],"scp":["runs:read","runs:write"],"organization_id":"org_custom_ns","https://usezombie.dev/tenant_id":"tenant_custom_ns","https://usezombie.dev/workspace_id":"ws_custom_ns"}
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
        if (result.role) |v| std.testing.allocator.free(v);
        if (result.audience) |v| std.testing.allocator.free(v);
        if (result.scopes) |v| std.testing.allocator.free(v);
    }
    try std.testing.expectEqualStrings("tenant_custom", result.tenant_id.?);
    try std.testing.expectEqualStrings("org_custom", result.org_id.?);
    try std.testing.expectEqualStrings("ws_custom", result.workspace_id.?);
    try std.testing.expect(result.role == null);
    try std.testing.expectEqualStrings("https://api.usezombie.com", result.audience.?);
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
        \\{"sub":"user_ns2","iss":"https://clerk.example.com","exp":9999999999,"https://usezombie.com/role":"admin"}
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
