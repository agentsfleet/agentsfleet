//! Claim normalization from verified JWT payloads.
//!
//! **One writer, one spelling.** `clerk_metadata_payload.zig` is the only thing
//! that ever writes our `public_metadata`, and it writes exactly two keys:
//! `tenant_id` and `scopes`. The identity provider's session-token claim
//! customization then projects `metadata.tenant_id` and a top-level `scopes`
//! (docs/AUTH.md §Clerk org config). Every reader below accepts precisely those
//! shapes and nothing else.
//!
//! It used to accept more — a second ladder for a `custom` provider reading
//! `custom_claims` / `app_metadata` / namespaced `https://agentsfleet.net/…`,
//! a `workspaceId` camelCase alias, and three spellings of the scope claim.
//! Nothing wrote any of them. The scope ladder was the dangerous one: it tried
//! OAuth2's `scope` BEFORE our own `scopes`, so a token carrying a standard
//! `scope` claim would have silently supplied a different capability set on the
//! authorisation path. A claim reader that accepts a fact from wherever it
//! might appear cannot say which value it trusted; these read one place each.

const std = @import("std");
const jwks = @import("jwks.zig");
const logging = @import("log");

const log = logging.scoped(.auth);

const S_METADATA = "metadata";
const S_MISSING = "missing";

pub const IdentityClaims = struct {
    tenant_id: ?[]u8,
    org_id: ?[]u8,
    workspace_id: ?[]u8,
    audience: ?[]u8,
    /// Space-delimited `resource:action` capability claim. The auth
    /// middleware parses it onto the principal as a bitset; the role ladder and
    /// `platform_admin` bool it replaced were removed in §4.
    scopes: ?[]u8,
};

const ClerkClaims = IdentityClaims;

const CLAIM_TENANT_ID = "tenant_id";
const CLAIM_ORG_ID = "org_id";
const CLAIM_WORKSPACE_ID = "workspace_id";
const CLAIM_SCOPES = "scopes";
const CLAIM_AUD = "aud";

/// Extract Clerk claims from a verified JWT payload.
/// Looks for `org_id` and `scopes` at top level, and `tenant_id`/`workspace_id`
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
        .audience = getAudience(parsed.value.object),
        .scopes = try getScopesOwned(alloc, parsed.value.object),
    });
}

fn parseClaimsObject(alloc: std.mem.Allocator, claims_json: []const u8) !std.json.Parsed(std.json.Value) {
    // OOM is a resource failure, not evidence the claims are malformed —
    // collapse only real parse errors (RULE ECL).
    const parsed = std.json.parseFromSlice(std.json.Value, alloc, claims_json, .{}) catch |err| switch (err) {
        error.OutOfMemory => return error.OutOfMemory,
        else => return jwks.VerifyError.TokenMalformed,
    };
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

    // One errdefer per acquisition: a failed later dupe frees every earlier
    // one instead of leaking it inside a half-built struct literal.
    const tenant_id = if (view.tenant_id) |v| try alloc.dupe(u8, v) else null;
    errdefer if (tenant_id) |v| alloc.free(v);
    const org_id = if (view.org_id) |v| try alloc.dupe(u8, v) else null;
    errdefer if (org_id) |v| alloc.free(v);
    const workspace_id = if (view.workspace_id) |v| try alloc.dupe(u8, v) else null;
    errdefer if (workspace_id) |v| alloc.free(v);
    const audience = if (view.audience) |v| try alloc.dupe(u8, v) else null;

    return .{
        .tenant_id = tenant_id,
        .org_id = org_id,
        .workspace_id = workspace_id,
        .audience = audience,
        .scopes = view.scopes,
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

    const metadata = obj.get(S_METADATA) orelse return null;
    if (metadata != .object) return null;
    return jwks.getString(metadata.object, CLAIM_WORKSPACE_ID);
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

/// The capability claim, read from `scopes` and nowhere else.
///
/// The space-delimited string is what the session-token template projects, and
/// what `defaultClaim` writes. The array form is accepted because a template
/// can be configured to emit one and the two mean the same thing — but it is
/// still the same single key, so there is never a question of which spelling
/// won.
fn getScopesOwned(alloc: std.mem.Allocator, obj: std.json.ObjectMap) !?[]u8 {
    if (jwks.getString(obj, CLAIM_SCOPES)) |v| return try alloc.dupe(u8, v);

    const raw = obj.get(CLAIM_SCOPES) orelse return null;
    if (raw != .array) return null;

    var buf: std.ArrayList(u8) = .empty;
    errdefer buf.deinit(alloc);
    for (raw.array.items) |item| {
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

test {
    _ = @import("claims_test.zig");
}
