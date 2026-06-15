//! Vendor-neutral OIDC verifier facade.
//! Supported adapters: Clerk and custom OIDC claim mappings.

const std = @import("std");
const jwks = @import("jwks.zig");
const claims = @import("claims.zig");
const logging = @import("log");
const MS_PER_SECOND = 1000;

const log = logging.scoped(.auth);

pub const Provider = enum {
    clerk,
    custom,
};

const ParseProviderError = error{
    InvalidProvider,
};

pub fn parseProvider(raw: []const u8) ParseProviderError!Provider {
    if (std.ascii.eqlIgnoreCase(raw, "clerk")) return .clerk;
    if (std.ascii.eqlIgnoreCase(raw, "custom")) return .custom;
    return ParseProviderError.InvalidProvider;
}

pub fn supportedProviderList() []const u8 {
    return "clerk, custom";
}

pub const VerifyError = jwks.VerifyError;

pub const Principal = struct {
    subject: []u8,
    issuer: []u8,
    tenant_id: ?[]u8,
    org_id: ?[]u8,
    workspace_id: ?[]u8,
    role: ?[]u8,
    audience: ?[]u8,
    scopes: ?[]u8,
    /// Platform-operator flag from the verified JWT. A bool, so it carries no
    /// allocation and needs no free. Defaults false (fail-closed).
    platform_admin: bool = false,
};

pub const Config = struct {
    provider: Provider = .clerk,
    jwks_url: []const u8,
    issuer: ?[]const u8 = null,
    audience: ?[]const u8 = null,
    inline_jwks_json: ?[]const u8 = null,
    cache_ttl_ms: i64 = 6 * 60 * 60 * MS_PER_SECOND,
};

pub const Verifier = struct {
    const Self = @This();

    provider: Provider,
    inner: jwks.Verifier,

    pub fn init(alloc: std.mem.Allocator, cfg: Config) Verifier {
        return .{
            .provider = cfg.provider,
            .inner = jwks.Verifier.init(alloc, .{
                .jwks_url = cfg.jwks_url,
                .issuer = cfg.issuer,
                .audience = cfg.audience,
                .inline_jwks_json = cfg.inline_jwks_json,
                .cache_ttl_ms = cfg.cache_ttl_ms,
            }),
        };
    }

    pub fn deinit(self: *Self) void {
        self.inner.deinit();
    }

    pub fn verifyAuthorization(self: *Self, alloc: std.mem.Allocator, authorization: []const u8) !Principal {
        log.debug("provider_selected", .{ .provider = @tagName(self.provider) });

        const verified = self.inner.verifyAndDecode(alloc, authorization) catch |err| {
            log.warn("verification_failed", .{ .err = @errorName(err) });
            return err;
        };
        errdefer {
            alloc.free(verified.subject);
            alloc.free(verified.issuer);
        }

        const normalized = switch (self.provider) {
            .clerk => try claims.extractClerkClaims(alloc, verified.claims_json),
            .custom => try claims.extractCustomClaims(alloc, verified.claims_json),
        };
        alloc.free(verified.claims_json);

        log.info("verification_ok", .{ .sub = verified.subject, .iss = verified.issuer });

        return .{
            .subject = verified.subject,
            .issuer = verified.issuer,
            .tenant_id = normalized.tenant_id,
            .org_id = normalized.org_id,
            .workspace_id = normalized.workspace_id,
            .role = normalized.role,
            .audience = normalized.audience,
            .scopes = normalized.scopes,
            .platform_admin = normalized.platform_admin,
        };
    }

    pub fn checkJwksConnectivity(self: *Self) !void {
        try self.inner.checkJwksConnectivity();
    }
};

const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","kid":"test-kid-static","use":"sig","alg":"RS256","n":"7ZUw6J4OYDXLJPGWADVw2-IgBawVd55H1Xh4R_FFFFYVNdG2O7EcTvBlFZhRzxDW9uL-SvxCt6slRDXDlZo9fmSI9yki7z8RAJZokcekxdP8za5w7g4QAoFeSieDhWWChkzHJ-vDGkrr0SAn8n4lIwpya-vCbO1eXmmz4Ay0pjenWyyGB1j371Zk2JGkAEJB347oJcVDMqVDt3d-TR0fyyspVw0nNxdDkZgNuB0EXOuEV4WvWgj0dtzwURhTI82AfpgheV23Kz7np9EoPxAhkfuslAjpRfqlRCXOOfmik-T6nvCe-fFPmHRwIY_zc1VrtwjKF0TjeALm4CCj_0pjRQ","e":"AQAB"}]}
;
const TEST_VALID_TOKEN =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLXN0YXRpYyJ9" ++ ".eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJpYXQiOjE3MDQwNjcyMDAsIm9yZ19pZCI6Im9yZ18xIiwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoidGVuYW50X2EifSwiZXhwIjo0MTAyNDQ0ODAwfQ" ++ ".pU5Y3T5yhLjleABex4K0fsyfjrxHDFa-8sjbI5hQhPHVw7P-WF_72VbWoCa9sVPi5cwGU0tbj8rZY2BMhq36_xZxwh7l4Z9SdguVGCiceDuqhhtRxA8vdPIlolrrykxAuEvlyeHRiE1uOzSvSGZZFCHvkgVK06SwC4oK1NlSgFx_cjKYbY0NychCG0XxLrl5XUoR79va4-9HGRMDYaTFRMutwMzFF_4iCbpn3RHl-qu9_RAabJrsQkeCmYYXaQKLt_aVVfrBMQWOwJDvCuTaeJcRGJefKmNdc-aM8mqBjZX9RIocD_hp5ADxY9HZdBFtGz7OAofgM2ZqVeJPkvNKfQ";

test "verifyAuthorization happy path via vendor-neutral oidc facade" {
    const providers = [_]Provider{ .clerk, .custom };
    for (providers) |provider| {
        var verifier = Verifier.init(std.testing.allocator, .{
            .provider = provider,
            .jwks_url = "https://clerk.dev.agentsfleet.net/.well-known/jwks.json",
            .issuer = "https://clerk.dev.agentsfleet.net",
            .audience = "https://api.agentsfleet.net",
            .inline_jwks_json = TEST_JWKS,
        });
        defer verifier.deinit();

        const principal = try verifier.verifyAuthorization(std.testing.allocator, "Bearer " ++ TEST_VALID_TOKEN);
        defer {
            std.testing.allocator.free(principal.subject);
            std.testing.allocator.free(principal.issuer);
            if (principal.tenant_id) |v| std.testing.allocator.free(v);
            if (principal.org_id) |v| std.testing.allocator.free(v);
            if (principal.workspace_id) |v| std.testing.allocator.free(v);
            if (principal.role) |v| std.testing.allocator.free(v);
            if (principal.audience) |v| std.testing.allocator.free(v);
            if (principal.scopes) |v| std.testing.allocator.free(v);
        }
        try std.testing.expectEqualStrings("tenant_a", principal.tenant_id.?);
        try std.testing.expectEqualStrings("https://api.agentsfleet.net", principal.audience.?);
    }
}

test "verifyAuthorization rejects invalid jwt_oidc token" {
    var verifier = Verifier.init(std.testing.allocator, .{
        .provider = .clerk,
        .jwks_url = "https://clerk.dev.agentsfleet.net/.well-known/jwks.json",
        .issuer = "https://clerk.dev.agentsfleet.net",
        .audience = "https://api.agentsfleet.net",
        .inline_jwks_json = TEST_JWKS,
    });
    defer verifier.deinit();

    // "invalid.token.value" has a 7-char base64 segment which has invalid padding;
    // decodeBase64UrlOwned maps that to TokenMalformed before authorization is checked.
    try std.testing.expectError(VerifyError.TokenMalformed, verifier.verifyAuthorization(std.testing.allocator, "Bearer invalid.token.value"));
}

test "parseProvider accepts supported adapters" {
    try std.testing.expectEqual(Provider.clerk, try parseProvider("clerk"));
    try std.testing.expectEqual(Provider.custom, try parseProvider("custom"));
}

test "parseProvider rejects invalid provider" {
    try std.testing.expectError(ParseProviderError.InvalidProvider, parseProvider("not-a-provider"));
}

test "parseProvider is case-insensitive and supportedProviderList is stable" {
    try std.testing.expectEqual(Provider.clerk, try parseProvider("CLERK"));
    try std.testing.expectEqual(Provider.custom, try parseProvider("Custom"));
    try std.testing.expectEqualStrings("clerk, custom", supportedProviderList());
}

test "verifyAuthorization returns null role when token has no role claim" {
    const providers = [_]Provider{ .clerk, .custom };
    for (providers) |provider| {
        var verifier = Verifier.init(std.testing.allocator, .{
            .provider = provider,
            .jwks_url = "https://clerk.dev.agentsfleet.net/.well-known/jwks.json",
            .issuer = "https://clerk.dev.agentsfleet.net",
            .audience = "https://api.agentsfleet.net",
            .inline_jwks_json = TEST_JWKS,
        });
        defer verifier.deinit();

        const principal = try verifier.verifyAuthorization(std.testing.allocator, "Bearer " ++ TEST_VALID_TOKEN);
        defer {
            std.testing.allocator.free(principal.subject);
            std.testing.allocator.free(principal.issuer);
            if (principal.tenant_id) |v| std.testing.allocator.free(v);
            if (principal.org_id) |v| std.testing.allocator.free(v);
            if (principal.workspace_id) |v| std.testing.allocator.free(v);
            if (principal.role) |v| std.testing.allocator.free(v);
            if (principal.audience) |v| std.testing.allocator.free(v);
            if (principal.scopes) |v| std.testing.allocator.free(v);
        }
        // The test token payload does not contain a role claim.
        try std.testing.expect(principal.role == null);
    }
}

test "Principal struct exposes role field alongside other identity fields" {
    const p = Principal{
        .subject = @constCast("sub"),
        .issuer = @constCast("iss"),
        .tenant_id = @constCast("t"),
        .org_id = @constCast("o"),
        .workspace_id = null,
        .role = @constCast("operator"),
        .audience = @constCast("aud"),
        .scopes = null,
    };
    try std.testing.expectEqualStrings("operator", p.role.?);
    try std.testing.expect(p.workspace_id == null);
    try std.testing.expect(p.scopes == null);
}

test "Principal struct role can be null" {
    const p = Principal{
        .subject = @constCast("sub"),
        .issuer = @constCast("iss"),
        .tenant_id = null,
        .org_id = null,
        .workspace_id = null,
        .role = null,
        .audience = null,
        .scopes = null,
    };
    try std.testing.expect(p.role == null);
}
