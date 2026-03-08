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

        return .{
            .subject = verified.subject,
            .issuer = verified.issuer,
            .tenant_id = clerk_claims.tenant_id,
            .org_id = clerk_claims.org_id,
        };
    }

    pub fn checkJwksConnectivity(self: *Verifier) !void {
        try self.inner.checkJwksConnectivity();
    }
};

// ── Test fixtures ──────────────────────────────────────────────────────

const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","kid":"test-kid-static","use":"sig","alg":"RS256","n":"kEge9Llezx-onM-jdO1fw85yTFmDDHWaZVdihVqMVAvRDGFvHbyoPrp5F-ZaDTqVEd1_pH12HM3abE6HRyYwSRxPcSKf2GlGWBVPtFbidOezLupgspHs8-yXBFKkGQEGBTWspJ4Obd0g9u1EX-cQqzy-lXiGd8gt1oK8Rxx5YBohNbaQMs5dbJ61J9c0afrG0dx-xOOx2tb95izx_m-sB83-aj7mX_r3ClpbZYcOY8ZKA3QNwR9tattkTiowpgzBZ0PGw5wuzrQayjWQRooolW4kzYMVWOI5K4GVPoabBDZDPs2nfet290iFHkNRu8cc2xPDmty0cDIhbS9Mq33qsQ","e":"AQAB"}]}
;
const TEST_VALID_TOKEN = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLXN0YXRpYyJ9.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImlhdCI6MTcwNDA2NzIwMCwib3JnX2lkIjoib3JnXzEiLCJtZXRhZGF0YSI6eyJ0ZW5hbnRfaWQiOiJ0ZW5hbnRfYSJ9LCJleHAiOjQxMDI0NDQ4MDB9.R5EaetratAEMN3VcDRDyR3KM9dKU3FYGEvzajPdmMUB_3T3qE0G0xZ_IoqyNilvjuMcbdSF-YQL1ylcMPTyBeFUWYAUlMjWBju-Bt3FF0Abqdte5-a64oPb_Ev0ogZyJcI8DDt9yT4kUjH7S2jp4fu9hQaEDMW_6tcASagCHTIjw2h0A41_Y8PI4CrgglIFqEKGim5PUEWM_KzZxs9pjv7-_HsZTovfZTcKeiJkGiFQvyR3oKfudvjLNyyGtdYKiSjfOWtLfJkxGt0CKPkbDbrnj_cSmwCt-X_v_OmG5vm07h7iDKrKhXiav0Djn7W3zZ8EcwjhlvMSsKZ3Uy9Nk2g";
const TEST_EXPIRED_TOKEN = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLXN0YXRpYyJ9.eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImlhdCI6MTcwNDA2NzIwMCwib3JnX2lkIjoib3JnXzEiLCJtZXRhZGF0YSI6eyJ0ZW5hbnRfaWQiOiJ0ZW5hbnRfYSJ9LCJleHAiOjE3MDQwNjczMDB9.CCjR252liw1fHwmo4kBmHH0nw1uPUBtibZx9BSPKPdzU_4oDmSrJyFP4LJtd9THDIVV8JyE5r1I9a8nuyLe66Wfr_N_tiiNAzYQ0voN_B2AQ-iy8DHhVAJibflv5eaGRXxh4pfn7uV3vY1ZGGDxwyjOXWPy_ULwSwtaDGDQNeWWYgVaaKp1B0-l__oIiMmRgsCiMOE6qyU2SFCQKG05vF54fgg7Pp4hpOgR9guE-rYKoLo39qE0RJvnaf5MTz2WbsPRxrvGurJ1lgnPrxGSXDMT2xJATkof6hP3Hv3QuSRlfCQwLEvlHKZG5ANpe7dxQ00KGf3RJiv0ly9mPapsD5g";

test "verifyAuthorization validates RS256 token and extracts tenant" {
    var verifier = Verifier.init(std.testing.allocator, .{
        .jwks_url = "https://clerk.dev.usezombie.com/.well-known/jwks.json",
        .issuer = "https://clerk.dev.usezombie.com",
        .audience = "https://api.usezombie.com",
        .inline_jwks_json = TEST_JWKS,
    });
    defer verifier.deinit();

    const auth = "Bearer " ++ TEST_VALID_TOKEN;
    const principal = try verifier.verifyAuthorization(std.testing.allocator, auth);
    defer {
        std.testing.allocator.free(principal.subject);
        std.testing.allocator.free(principal.issuer);
        if (principal.tenant_id) |v| std.testing.allocator.free(v);
        if (principal.org_id) |v| std.testing.allocator.free(v);
    }

    try std.testing.expectEqualStrings("user_test", principal.subject);
    try std.testing.expectEqualStrings("https://clerk.dev.usezombie.com", principal.issuer);
    try std.testing.expectEqualStrings("tenant_a", principal.tenant_id.?);
    try std.testing.expectEqualStrings("org_1", principal.org_id.?);
}

test "verifyAuthorization rejects expired token" {
    var verifier = Verifier.init(std.testing.allocator, .{
        .jwks_url = "https://clerk.dev.usezombie.com/.well-known/jwks.json",
        .issuer = "https://clerk.dev.usezombie.com",
        .audience = "https://api.usezombie.com",
        .inline_jwks_json = TEST_JWKS,
    });
    defer verifier.deinit();

    const auth = "Bearer " ++ TEST_EXPIRED_TOKEN;
    try std.testing.expectError(VerifyError.TokenExpired, verifier.verifyAuthorization(std.testing.allocator, auth));
}

test "integration: audience mismatch is rejected" {
    var verifier = Verifier.init(std.testing.allocator, .{
        .jwks_url = "https://clerk.dev.usezombie.com/.well-known/jwks.json",
        .issuer = "https://clerk.dev.usezombie.com",
        .audience = "https://api.other.example",
        .inline_jwks_json = TEST_JWKS,
    });
    defer verifier.deinit();

    const auth = "Bearer " ++ TEST_VALID_TOKEN;
    try std.testing.expectError(VerifyError.AudienceMismatch, verifier.verifyAuthorization(std.testing.allocator, auth));
}
