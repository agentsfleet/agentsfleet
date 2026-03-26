//! Vendor-neutral OIDC verifier facade.
//! Supported adapters: Clerk and custom OIDC claim mappings.

const std = @import("std");
const jwks = @import("jwks.zig");
const claims = @import("claims.zig");

const log = std.log.scoped(.auth);

pub const Provider = enum {
    clerk,
    custom,
};

pub const ParseProviderError = error{
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
};

pub const Config = struct {
    provider: Provider = .clerk,
    jwks_url: []const u8,
    issuer: ?[]const u8 = null,
    audience: ?[]const u8 = null,
    inline_jwks_json: ?[]const u8 = null,
    cache_ttl_ms: i64 = 6 * 60 * 60 * 1000,
};

pub const Verifier = struct {
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

    pub fn deinit(self: *Verifier) void {
        self.inner.deinit();
    }

    pub fn verifyAuthorization(self: *Verifier, alloc: std.mem.Allocator, authorization: []const u8) !Principal {
        log.debug("provider={s}", .{@tagName(self.provider)});

        const verified = self.inner.verifyAndDecode(alloc, authorization) catch |err| {
            log.warn("verification failed err={s}", .{@errorName(err)});
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

        log.info("verification ok sub={s} iss={s}", .{ verified.subject, verified.issuer });

        return .{
            .subject = verified.subject,
            .issuer = verified.issuer,
            .tenant_id = normalized.tenant_id,
            .org_id = normalized.org_id,
            .workspace_id = normalized.workspace_id,
            .role = normalized.role,
            .audience = normalized.audience,
            .scopes = normalized.scopes,
        };
    }

    pub fn checkJwksConnectivity(self: *Verifier) !void {
        try self.inner.checkJwksConnectivity();
    }
};

const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","kid":"test-kid-static","use":"sig","alg":"RS256","n":"kEge9Llezx-onM-jdO1fw85yTFmDDHWaZVdihVqMVAvRDGFvHbyoPrp5F-ZaDTqVEd1_pH12HM3abE6HRyYwSRxPcSKf2GlGWBVPtFbidOezLupgspHs8-yXBFKkGQEGBTWspJ4Obd0g9u1EX-cQqzy-lXiGd8gt1oK8Rxx5YBohNbaQMs5dbJ61J9c0afrG0dx-xOOx2tb95izx_m-sB83-aj7mX_r3ClpbZYcOY8ZKA3QNwR9tattkTiowpgzBZ0PGw5wuzrQayjWQRooolW4kzYMVWOI5K4GVPoabBDZDPs2nfet290iFHkNRu8cc2xPDmty0cDIhbS9Mq33qsQ","e":"AQAB"}]}
;
const TEST_VALID_TOKEN =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLXN0YXRpYyJ9" ++ ".eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImlhdCI6MTcwNDA2NzIwMCwib3JnX2lkIjoib3JnXzEiLCJtZXRhZGF0YSI6eyJ0ZW5hbnRfaWQiOiJ0ZW5hbnRfYSJ9LCJleHAiOjQxMDI0NDQ4MDB9" ++ ".R5EaetratAEMN3VcDRDyR3KM9dKU3FYGEvzajPdmMUB_3T3qE0G0xZ_IoqyNilvjuMcbdSF-YQL1ylcMPTyBeFUWYAUlMjWBju-Bt3FF0Abqdte5-a64oPb_Ev0ogZyJcI8DDt9yT4kUjH7S2jp4fu9hQaEDMW_6tcASagCHTIjw2h0A41_Y8PI4CrgglIFqEKGim5PUEWM_KzZxs9pjv7-_HsZTovfZTcKeiJkGiFQvyR3oKfudvjLNyyGtdYKiSjfOWtLfJkxGt0CKPkbDbrnj_cSmwCt-X_v_OmG5vm07h7iDKrKhXiav0Djn7W3zZ8EcwjhlvMSsKZ3Uy9Nk2g";

test "verifyAuthorization happy path via vendor-neutral oidc facade" {
    const providers = [_]Provider{ .clerk, .custom };
    for (providers) |provider| {
        var verifier = Verifier.init(std.testing.allocator, .{
            .provider = provider,
            .jwks_url = "https://clerk.dev.usezombie.com/.well-known/jwks.json",
            .issuer = "https://clerk.dev.usezombie.com",
            .audience = "https://api.usezombie.com",
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
        try std.testing.expectEqualStrings("https://api.usezombie.com", principal.audience.?);
    }
}

test "verifyAuthorization rejects invalid jwt_oidc token" {
    var verifier = Verifier.init(std.testing.allocator, .{
        .provider = .clerk,
        .jwks_url = "https://clerk.dev.usezombie.com/.well-known/jwks.json",
        .issuer = "https://clerk.dev.usezombie.com",
        .audience = "https://api.usezombie.com",
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
            .jwks_url = "https://clerk.dev.usezombie.com/.well-known/jwks.json",
            .issuer = "https://clerk.dev.usezombie.com",
            .audience = "https://api.usezombie.com",
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
