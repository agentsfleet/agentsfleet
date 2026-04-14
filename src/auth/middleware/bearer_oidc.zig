//! `bearer_oidc` middleware (M18_002 §3.1).
//!
//! Verifies `Authorization: Bearer <jwt>` via a host-supplied `oidc.Verifier`.
//! On success populates `ctx.principal` with mode=`.jwt_oidc`, role derived
//! from the normalized claims, and user/tenant/workspace IDs from the JWT.
//!
//! Scope note vs M11_002 `common.authenticate`: this middleware does NOT run
//! the `id_format.isSupportedTenantId/WorkspaceId` shape checks — those live
//! in `src/types/id_format.zig` which `src/auth/` can't import without
//! violating §1.2. Handlers continue to validate IDs they care about. The
//! gap is closed by Batch D handler-level checks where relevant.

const std = @import("std");
const httpz = @import("httpz");

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");
const bearer = @import("bearer.zig");
const errors = @import("errors.zig");
const oidc = @import("../oidc.zig");
const rbac = @import("../rbac.zig");
const principal_mod = @import("../principal.zig");

pub const AuthCtx = auth_ctx.AuthCtx;

/// Free fields of `oidc.Principal` that `AuthPrincipal` does not adopt.
/// `AuthPrincipal` keeps subject/tenant_id/workspace_id; everything else
/// (issuer, org_id, role, audience, scopes) would otherwise leak.
fn freeUnusedPrincipalFields(alloc: std.mem.Allocator, p: oidc.Principal) void {
    alloc.free(p.issuer);
    if (p.org_id) |v| alloc.free(v);
    if (p.role) |v| alloc.free(v);
    if (p.audience) |v| alloc.free(v);
    if (p.scopes) |v| alloc.free(v);
}

pub const BearerOidc = struct {
    verifier: *oidc.Verifier,

    pub fn middleware(self: *BearerOidc) chain.Middleware(AuthCtx) {
        return .{ .ptr = self, .execute_fn = executeTypeErased };
    }

    fn executeTypeErased(ptr: *anyopaque, ctx: *AuthCtx, req: *httpz.Request) anyerror!chain.Outcome {
        const self: *BearerOidc = @ptrCast(@alignCast(ptr));
        return execute(self, ctx, req);
    }

    pub fn execute(self: *BearerOidc, ctx: *AuthCtx, req: *httpz.Request) !chain.Outcome {
        if (bearer.parseBearerToken(req) == null) {
            ctx.fail(errors.ERR_UNAUTHORIZED, "Invalid or missing token");
            return .short_circuit;
        }
        const auth_header = req.header("authorization").?;
        const verified = self.verifier.verifyAuthorization(ctx.alloc, auth_header) catch |err| switch (err) {
            error.TokenExpired => {
                ctx.fail(errors.ERR_TOKEN_EXPIRED, "token expired");
                return .short_circuit;
            },
            error.JwksFetchFailed, error.JwksParseFailed => {
                ctx.fail(errors.ERR_AUTH_UNAVAILABLE, "Authentication service unavailable");
                return .short_circuit;
            },
            else => {
                ctx.fail(errors.ERR_UNAUTHORIZED, "Invalid or missing token");
                return .short_circuit;
            },
        };
        // SAFETY: claims.zig normalizes role strings through rbac.parseAuthRole
        // during claim extraction. Any non-null value should map; if the raw
        // label is somehow invalid, surface UnsupportedRole (403) — never
        // silently treat as .user.
        const role: principal_mod.AuthRole = if (verified.role) |raw|
            rbac.parseAuthRole(raw) orelse {
                freeUnusedPrincipalFields(ctx.alloc, verified);
                ctx.alloc.free(verified.subject);
                if (verified.tenant_id) |v| ctx.alloc.free(v);
                if (verified.workspace_id) |v| ctx.alloc.free(v);
                ctx.fail(errors.ERR_UNSUPPORTED_ROLE, "Unsupported role in token");
                return .short_circuit;
            }
        else
            .user;
        // AuthPrincipal captures subject/tenant_id/workspace_id; free the rest
        // so we don't leak the extra oidc.Principal fields.
        freeUnusedPrincipalFields(ctx.alloc, verified);
        ctx.principal = .{
            .mode = .jwt_oidc,
            .role = role,
            .user_id = verified.subject,
            .tenant_id = verified.tenant_id,
            .workspace_scope_id = verified.workspace_id,
        };
        return .next;
    }
};

// ── Tests ────────────────────────────────────────────────────────────────
//
// Uses the same inline JWKS + signed RS256 test token the clerk_test suite
// already exercises, so we can run the real verifier end-to-end under the
// auth-only target without needing a mock.

const testing = std.testing;

const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","kid":"test-kid-static","use":"sig","alg":"RS256","n":"kEge9Llezx-onM-jdO1fw85yTFmDDHWaZVdihVqMVAvRDGFvHbyoPrp5F-ZaDTqVEd1_pH12HM3abE6HRyYwSRxPcSKf2GlGWBVPtFbidOezLupgspHs8-yXBFKkGQEGBTWspJ4Obd0g9u1EX-cQqzy-lXiGd8gt1oK8Rxx5YBohNbaQMs5dbJ61J9c0afrG0dx-xOOx2tb95izx_m-sB83-aj7mX_r3ClpbZYcOY8ZKA3QNwR9tattkTiowpgzBZ0PGw5wuzrQayjWQRooolW4kzYMVWOI5K4GVPoabBDZDPs2nfet290iFHkNRu8cc2xPDmty0cDIhbS9Mq33qsQ","e":"AQAB"}]}
;
const TEST_HEADER = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLXN0YXRpYyJ9";
const TEST_PAYLOAD_VALID = "eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi51c2V6b21iaWUuY29tIiwiYXVkIjoiaHR0cHM6Ly9hcGkudXNlem9tYmllLmNvbSIsImlhdCI6MTcwNDA2NzIwMCwib3JnX2lkIjoib3JnXzEiLCJtZXRhZGF0YSI6eyJ0ZW5hbnRfaWQiOiJ0ZW5hbnRfYSJ9LCJleHAiOjQxMDI0NDQ4MDB9";
const TEST_SIG_VALID = "R5EaetratAEMN3VcDRDyR3KM9dKU3FYGEvzajPdmMUB_3T3qE0G0xZ_IoqyNilvjuMcbdSF-YQL1ylcMPTyBeFUWYAUlMjWBju-Bt3FF0Abqdte5-a64oPb_Ev0ogZyJcI8DDt9yT4kUjH7S2jp4fu9hQaEDMW_6tcASagCHTIjw2h0A41_Y8PI4CrgglIFqEKGim5PUEWM_KzZxs9pjv7-_HsZTovfZTcKeiJkGiFQvyR3oKfudvjLNyyGtdYKiSjfOWtLfJkxGt0CKPkbDbrnj_cSmwCt-X_v_OmG5vm07h7iDKrKhXiav0Djn7W3zZ8EcwjhlvMSsKZ3Uy9Nk2g";
const TEST_VALID_TOKEN = TEST_HEADER ++ "." ++ TEST_PAYLOAD_VALID ++ "." ++ TEST_SIG_VALID;

const test_fixtures = struct {
    var last_code: []const u8 = "";
    var write_count: usize = 0;

    fn reset() void {
        last_code = "";
        write_count = 0;
    }

    fn writeError(_: *httpz.Response, code: []const u8, _: []const u8, _: []const u8) void {
        last_code = code;
        write_count += 1;
    }
};

fn makeVerifier() oidc.Verifier {
    return oidc.Verifier.init(testing.allocator, .{
        .provider = .clerk,
        .jwks_url = "https://clerk.dev.usezombie.com/.well-known/jwks.json",
        .issuer = "https://clerk.dev.usezombie.com",
        .audience = "https://api.usezombie.com",
        .inline_jwks_json = TEST_JWKS,
    });
}

fn runOne(mw: *BearerOidc, ht: anytype) !struct { outcome: chain.Outcome, ctx: AuthCtx } {
    var ctx = AuthCtx{
        .alloc = testing.allocator,
        .res = ht.res,
        .req_id = "req_test",
        .write_error = test_fixtures.writeError,
    };
    const outcome = try mw.execute(&ctx, ht.req);
    return .{ .outcome = outcome, .ctx = ctx };
}

test "bearer_oidc .next + JWT principal on valid signed RS256 token" {
    test_fixtures.reset();
    var verifier = makeVerifier();
    defer verifier.deinit();

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer " ++ TEST_VALID_TOKEN);

    var mw = BearerOidc{ .verifier = &verifier };
    const result = try runOne(&mw, &ht);
    // Free allocations the Principal took ownership of.
    defer if (result.ctx.principal) |p| {
        if (p.user_id) |v| testing.allocator.free(v);
        if (p.tenant_id) |v| testing.allocator.free(v);
        if (p.workspace_scope_id) |v| testing.allocator.free(v);
    };

    try testing.expectEqual(chain.Outcome.next, result.outcome);
    try testing.expectEqual(@as(usize, 0), test_fixtures.write_count);
    try testing.expect(result.ctx.principal != null);
    try testing.expectEqual(principal_mod.AuthMode.jwt_oidc, result.ctx.principal.?.mode);
    try testing.expectEqualStrings("user_test", result.ctx.principal.?.user_id.?);
    try testing.expectEqualStrings("tenant_a", result.ctx.principal.?.tenant_id.?);
}

test "bearer_oidc short-circuits with 401 when Authorization header is missing" {
    test_fixtures.reset();
    var verifier = makeVerifier();
    defer verifier.deinit();

    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    var mw = BearerOidc{ .verifier = &verifier };
    const result = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.short_circuit, result.outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, test_fixtures.last_code);
    try testing.expect(result.ctx.principal == null);
}

test "bearer_oidc short-circuits with 401 on malformed JWT" {
    test_fixtures.reset();
    var verifier = makeVerifier();
    defer verifier.deinit();

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer not.a.jwt");

    var mw = BearerOidc{ .verifier = &verifier };
    const result = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.short_circuit, result.outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, test_fixtures.last_code);
}

test "bearer_oidc short-circuits with 503 on JWKS fetch failure" {
    test_fixtures.reset();
    // Verifier with a bogus JWKS URL and no inline keys → JwksFetchFailed.
    var verifier = oidc.Verifier.init(testing.allocator, .{
        .provider = .clerk,
        .jwks_url = "http://127.0.0.1:1/unreachable.json",
        .issuer = "https://clerk.dev.usezombie.com",
        .audience = "https://api.usezombie.com",
    });
    defer verifier.deinit();

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer " ++ TEST_VALID_TOKEN);

    var mw = BearerOidc{ .verifier = &verifier };
    const result = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.short_circuit, result.outcome);
    try testing.expectEqualStrings(errors.ERR_AUTH_UNAVAILABLE, test_fixtures.last_code);
}
