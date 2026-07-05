//! `bearer_or_api_key` middleware.
//!
//! Accepts a valid OIDC JWT or a tenant-minted `agt_t` API key via
//! `Authorization: Bearer <token>`. The env-var `API_KEY` bootstrap path
//! was deleted in M11_006 — there is no longer a global admin-by-env-var
//! principal. Admin gating is scope-based (see docs/AUTH.md's Scope
//! catalogue): a top-level `scopes` claim, projected by the Clerk
//! session-token template from `public_metadata.scopes`.
//!
//! Resolution order:
//!   1. Bearer token is parsed.
//!   2. If prefixed `agt_t` → DB-backed tenant_api_key lookup.
//!   3. Else if `verifier` is configured → JWT verification path.
//!   4. Else → 401.

const std = @import("std");
const httpz = @import("httpz");

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");
const bearer = @import("bearer.zig");
const errors = @import("errors.zig");
const oidc = @import("../oidc.zig");
const scopes = @import("../scopes.zig");
const principal_mod = @import("../principal.zig");
const tenant_api_key_mod = @import("tenant_api_key.zig");

pub const AuthCtx = auth_ctx.AuthCtx;
pub const TenantApiKey = tenant_api_key_mod.TenantApiKey;

const S_INVALID_OR_MISSING_TOKEN = "Invalid or missing token";

/// Free fields of `oidc.Principal` that `AuthPrincipal` does not adopt —
/// keeps subject/tenant_id/workspace_id; issuer/org_id/audience/scopes
/// would otherwise leak. (The `scopes` string is freed here because the
/// principal adopts its parsed bitset, not the raw string.)
fn freeUnusedPrincipalFields(alloc: std.mem.Allocator, p: oidc.Principal) void {
    alloc.free(p.issuer);
    if (p.org_id) |v| alloc.free(v);
    if (p.audience) |v| alloc.free(v);
    if (p.scopes) |v| alloc.free(v);
}

pub const BearerOrApiKey = struct {
    const Self = @This();

    verifier: ?*oidc.Verifier,
    /// Populated by MiddlewareRegistry.initChains() when a tenant API-key
    /// lookup is wired. When set, any `agt_t`-prefixed Bearer token is
    /// routed to the tenant-key path (DB-backed lookup via host callback).
    tenant_api_key: ?*TenantApiKey = null,

    pub fn middleware(self: *Self) chain.Middleware(AuthCtx) {
        return .{ .ptr = self, .execute_fn = executeTypeErased };
    }

    fn executeTypeErased(ptr: *anyopaque, ctx: *AuthCtx, req: *httpz.Request) anyerror!chain.Outcome {
        const self: *BearerOrApiKey = @ptrCast(@alignCast(ptr));
        return execute(self, ctx, req);
    }

    pub fn execute(self: *Self, ctx: *AuthCtx, req: *httpz.Request) !chain.Outcome {
        const provided = bearer.parseBearerToken(req) orelse {
            ctx.fail(errors.ERR_UNAUTHORIZED, S_INVALID_OR_MISSING_TOKEN);
            return .short_circuit;
        };

        if (self.tenant_api_key) |tapi| {
            if (std.mem.startsWith(u8, provided, tenant_api_key_mod.TENANT_KEY_PREFIX)) {
                return tapi.execute(ctx, req);
            }
        }

        const verifier = self.verifier orelse {
            ctx.fail(errors.ERR_UNAUTHORIZED, S_INVALID_OR_MISSING_TOKEN);
            return .short_circuit;
        };

        const auth_header = req.header("authorization").?;
        const verified = verifier.verifyAuthorization(ctx.alloc, auth_header) catch |err| switch (err) {
            error.TokenExpired => {
                ctx.fail(errors.ERR_TOKEN_EXPIRED, "token expired");
                return .short_circuit;
            },
            error.JwksFetchFailed, error.JwksParseFailed => {
                ctx.fail(errors.ERR_AUTH_UNAVAILABLE, "Authentication service unavailable");
                return .short_circuit;
            },
            else => {
                ctx.fail(errors.ERR_UNAUTHORIZED, S_INVALID_OR_MISSING_TOKEN);
                return .short_circuit;
            },
        };
        // Parse the explicit scope claim into a bitset BEFORE freeing the source
        // string (parseClaim copies into the EnumSet — no borrow survives).
        const scope_set = if (verified.scopes) |s| scopes.parseClaim(s) else scopes.Set.initEmpty();
        // AuthPrincipal adopts subject/tenant_id/workspace_id; free the rest.
        freeUnusedPrincipalFields(ctx.alloc, verified);
        ctx.principal = .{
            .mode = .jwt_oidc,
            .user_id = verified.subject,
            .tenant_id = verified.tenant_id,
            .workspace_scope_id = verified.workspace_id,
            .scopes = scope_set,
        };
        return .next;
    }
};

// ── Tests ────────────────────────────────────────────────────────────────

const testing = std.testing;

const TEST_JWKS =
    \\{"keys":[{"kty":"RSA","kid":"test-kid-static","use":"sig","alg":"RS256","n":"7ZUw6J4OYDXLJPGWADVw2-IgBawVd55H1Xh4R_FFFFYVNdG2O7EcTvBlFZhRzxDW9uL-SvxCt6slRDXDlZo9fmSI9yki7z8RAJZokcekxdP8za5w7g4QAoFeSieDhWWChkzHJ-vDGkrr0SAn8n4lIwpya-vCbO1eXmmz4Ay0pjenWyyGB1j371Zk2JGkAEJB347oJcVDMqVDt3d-TR0fyyspVw0nNxdDkZgNuB0EXOuEV4WvWgj0dtzwURhTI82AfpgheV23Kz7np9EoPxAhkfuslAjpRfqlRCXOOfmik-T6nvCe-fFPmHRwIY_zc1VrtwjKF0TjeALm4CCj_0pjRQ","e":"AQAB"}]}
;
const TEST_HEADER = "eyJhbGciOiJSUzI1NiIsInR5cCI6IkpXVCIsImtpZCI6InRlc3Qta2lkLXN0YXRpYyJ9";
const TEST_PAYLOAD_VALID = "eyJzdWIiOiJ1c2VyX3Rlc3QiLCJpc3MiOiJodHRwczovL2NsZXJrLmRldi5hZ2VudHNmbGVldC5uZXQiLCJhdWQiOiJodHRwczovL2FwaS5hZ2VudHNmbGVldC5uZXQiLCJpYXQiOjE3MDQwNjcyMDAsIm9yZ19pZCI6Im9yZ18xIiwibWV0YWRhdGEiOnsidGVuYW50X2lkIjoidGVuYW50X2EifSwiZXhwIjo0MTAyNDQ0ODAwfQ";
const TEST_SIG_VALID = "pU5Y3T5yhLjleABex4K0fsyfjrxHDFa-8sjbI5hQhPHVw7P-WF_72VbWoCa9sVPi5cwGU0tbj8rZY2BMhq36_xZxwh7l4Z9SdguVGCiceDuqhhtRxA8vdPIlolrrykxAuEvlyeHRiE1uOzSvSGZZFCHvkgVK06SwC4oK1NlSgFx_cjKYbY0NychCG0XxLrl5XUoR79va4-9HGRMDYaTFRMutwMzFF_4iCbpn3RHl-qu9_RAabJrsQkeCmYYXaQKLt_aVVfrBMQWOwJDvCuTaeJcRGJefKmNdc-aM8mqBjZX9RIocD_hp5ADxY9HZdBFtGz7OAofgM2ZqVeJPkvNKfQ";
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
        .jwks_url = "https://clerk.dev.agentsfleet.net/.well-known/jwks.json",
        .issuer = "https://clerk.dev.agentsfleet.net",
        .audience = "https://api.agentsfleet.net",
        .inline_jwks_json = TEST_JWKS,
    });
}

fn runOne(mw: *BearerOrApiKey, ht: anytype) !struct { outcome: chain.Outcome, ctx: AuthCtx } {
    var ctx = AuthCtx{
        .alloc = testing.allocator,
        .res = ht.res,
        .req_id = "req_test",
        .write_error = test_fixtures.writeError,
    };
    const outcome = try mw.execute(&ctx, ht.req);
    return .{ .outcome = outcome, .ctx = ctx };
}

test "bearer_or_api_key routes a valid JWT to the OIDC path" {
    test_fixtures.reset();
    var verifier = makeVerifier();
    defer verifier.deinit();

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer " ++ TEST_VALID_TOKEN);

    var mw = BearerOrApiKey{ .verifier = &verifier };
    const result = try runOne(&mw, &ht);
    defer if (result.ctx.principal) |p| {
        if (p.user_id) |v| testing.allocator.free(v);
        if (p.tenant_id) |v| testing.allocator.free(v);
        if (p.workspace_scope_id) |v| testing.allocator.free(v);
    };

    try testing.expectEqual(chain.Outcome.next, result.outcome);
    try testing.expect(result.ctx.principal != null);
    try testing.expectEqual(principal_mod.AuthMode.jwt_oidc, result.ctx.principal.?.mode);
    try testing.expectEqualStrings("user_test", result.ctx.principal.?.user_id.?);
}

test "bearer_or_api_key short-circuits with 401 when Authorization header is missing" {
    test_fixtures.reset();
    var verifier = makeVerifier();
    defer verifier.deinit();

    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    var mw = BearerOrApiKey{ .verifier = &verifier };
    const result = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.short_circuit, result.outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, test_fixtures.last_code);
}

test "bearer_or_api_key short-circuits with 401 when no verifier is configured" {
    test_fixtures.reset();
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer something-else");

    var mw = BearerOrApiKey{ .verifier = null };
    const result = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.short_circuit, result.outcome);
    try testing.expectEqualStrings(errors.ERR_UNAUTHORIZED, test_fixtures.last_code);
}

test "bearer_or_api_key short-circuits with 503 when JWKS fetch fails" {
    test_fixtures.reset();
    var verifier = oidc.Verifier.init(testing.allocator, .{
        .provider = .clerk,
        .jwks_url = "http://127.0.0.1:1/unreachable.json",
        .issuer = "https://clerk.dev.agentsfleet.net",
        .audience = "https://api.agentsfleet.net",
    });
    defer verifier.deinit();

    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    ht.header("authorization", "Bearer " ++ TEST_VALID_TOKEN);

    var mw = BearerOrApiKey{ .verifier = &verifier };
    const result = try runOne(&mw, &ht);

    try testing.expectEqual(chain.Outcome.short_circuit, result.outcome);
    try testing.expectEqualStrings(errors.ERR_AUTH_UNAVAILABLE, test_fixtures.last_code);
}
