//! Clerk identity provider — thin wrapper around generic JWKS verifier
//! plus Clerk-specific claim extraction.
//!
//! Swapping to another OIDC provider means writing a new claims extractor
//! and a similar thin wrapper; the JWKS core stays unchanged.

const std = @import("std");
const jwks = @import("jwks.zig");
const claims = @import("claims.zig");

pub const VerifyError = jwks.VerifyError;

pub const Principal = struct {
    subject: []u8,
    issuer: []u8,
    tenant_id: ?[]u8,
    org_id: ?[]u8,
    workspace_id: ?[]u8,
    role: ?[]u8,
};

pub const Config = struct {
    jwks_url: []const u8,
    issuer: ?[]const u8 = null,
    audience: ?[]const u8 = null,
    inline_jwks_json: ?[]const u8 = null,
    cache_ttl_ms: i64 = 6 * 60 * 60 * 1000,
};

pub const Verifier = struct {
    inner: jwks.Verifier,

    pub fn init(alloc: std.mem.Allocator, cfg: Config) Verifier {
        return .{
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
        const verified = try self.inner.verifyAndDecode(alloc, authorization);
        errdefer {
            alloc.free(verified.subject);
            alloc.free(verified.issuer);
        }

        const clerk_claims = try claims.extractClerkClaims(alloc, verified.claims_json);
        // claims_json ownership was transferred from verifyAndDecode;
        // parseStandardClaims does not free it, so we must.
        alloc.free(verified.claims_json);
        // Principal only carries 4 of the IdentityClaims fields; free the rest
        // or they leak through the standardised allocator on test runs.
        if (clerk_claims.audience) |v| alloc.free(v);
        if (clerk_claims.scopes) |v| alloc.free(v);

        return .{
            .subject = verified.subject,
            .issuer = verified.issuer,
            .tenant_id = clerk_claims.tenant_id,
            .org_id = clerk_claims.org_id,
            .workspace_id = clerk_claims.workspace_id,
            .role = clerk_claims.role,
        };
    }

    pub fn checkJwksConnectivity(self: *Verifier) !void {
        try self.inner.checkJwksConnectivity();
    }
};

test {
    _ = @import("./clerk_test.zig");
}
