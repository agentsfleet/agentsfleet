//! Vendor-neutral OIDC verifier facade.
//! Current provider implementation: Clerk.

const std = @import("std");
const clerk = @import("clerk.zig");

pub const Provider = enum {
    clerk,
};

pub const ParseProviderError = error{
    InvalidProvider,
};

pub fn parseProvider(raw: []const u8) ParseProviderError!Provider {
    if (std.ascii.eqlIgnoreCase(raw, "clerk")) return .clerk;
    return ParseProviderError.InvalidProvider;
}

pub const VerifyError = clerk.VerifyError;

pub const Principal = struct {
    subject: []u8,
    issuer: []u8,
    tenant_id: ?[]u8,
    org_id: ?[]u8,
    workspace_id: ?[]u8,
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
    clerk_verifier: ?clerk.Verifier = null,

    pub fn init(alloc: std.mem.Allocator, cfg: Config) Verifier {
        return switch (cfg.provider) {
            .clerk => .{
                .provider = .clerk,
                .clerk_verifier = clerk.Verifier.init(alloc, .{
                    .jwks_url = cfg.jwks_url,
                    .issuer = cfg.issuer,
                    .audience = cfg.audience,
                    .inline_jwks_json = cfg.inline_jwks_json,
                    .cache_ttl_ms = cfg.cache_ttl_ms,
                }),
            },
        };
    }

    pub fn deinit(self: *Verifier) void {
        switch (self.provider) {
            .clerk => if (self.clerk_verifier) |*v| v.deinit(),
        }
    }

    pub fn verifyAuthorization(self: *Verifier, alloc: std.mem.Allocator, authorization: []const u8) !Principal {
        return switch (self.provider) {
            .clerk => {
                const v = self.clerk_verifier orelse return VerifyError.TokenMalformed;
                const p = try v.verifyAuthorization(alloc, authorization);
                return .{
                    .subject = p.subject,
                    .issuer = p.issuer,
                    .tenant_id = p.tenant_id,
                    .org_id = p.org_id,
                    .workspace_id = p.workspace_id,
                };
            },
        };
    }

    pub fn checkJwksConnectivity(self: *Verifier) !void {
        switch (self.provider) {
            .clerk => {
                const v = self.clerk_verifier orelse return VerifyError.JwksFetchFailed;
                try v.checkJwksConnectivity();
            },
        }
    }
};

const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","kid":"test-kid-static","use":"sig","alg":"RS256","n":"kEge9Llezx-onM-jdO1fw85yTFmDDHWaZVdihVqMVAvRDGFvHbyoPrp5F-ZaDTqVEd1_pH12HM3abE6HRyYwSRxPcSKf2GlGWBVPtFbidOezLupgspHs8-yXBFKkGQEGBTWspJ4Obd0g9u1EX-cQqzy-lXiGd8gt1oK8Rxx5YBohNbaQMs5dbJ61J9c0afrG0dx-xOOx2tb95izx_m-sB83-aj7mX_r3ClpbZYcOY8ZKA3QNwR9tattkTiowpgzBZ0PGw5wuzrQayjWQRooolW4kzYMVWOI5K4GVPoabBDZDPs2nfet290iFHkNRu8cc2xPDmty0cDIhbS9Mq33qsQ","e":"AQAB"}]}
;
const TEST_VALID_TOKEN =
    "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLXN0YXRpYyJ9" ++ ".eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImlhdCI6MTcwNDA2NzIwMCwib3JnX2lkIjoib3JnXzEiLCJtZXRhZGF0YSI6eyJ0ZW5hbnRfaWQiOiJ0ZW5hbnRfYSJ9LCJleHAiOjQxMDI0NDQ4MDB9" ++ ".R5EaetratAEMN3VcDRDyR3KM9dKU3FYGEvzajPdmMUB_3T3qE0G0xZ_IoqyNilvjuMcbdSF-YQL1ylcMPTyBeFUWYAUlMjWBju-Bt3FF0Abqdte5-a64oPb_Ev0ogZyJcI8DDt9yT4kUjH7S2jp4fu9hQaEDMW_6tcASagCHTIjw2h0A41_Y8PI4CrgglIFqEKGim5PUEWM_KzZxs9pjv7-_HsZTovfZTcKeiJkGiFQvyR3oKfudvjLNyyGtdYKiSjfOWtLfJkxGt0CKPkbDbrnj_cSmwCt-X_v_OmG5vm07h7iDKrKhXiav0Djn7W3zZ8EcwjhlvMSsKZ3Uy9Nk2g";

test "verifyAuthorization happy path via vendor-neutral oidc facade" {
    var verifier = Verifier.init(std.testing.allocator, .{
        .provider = .clerk,
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
    }
    try std.testing.expectEqualStrings("tenant_a", principal.tenant_id.?);
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

    try std.testing.expectError(VerifyError.InvalidAuthorization, verifier.verifyAuthorization(std.testing.allocator, "Bearer invalid.token.value"));
}

test "parseProvider rejects invalid provider" {
    try std.testing.expectError(ParseProviderError.InvalidProvider, parseProvider("not-a-provider"));
}
