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
//!   3. If prefixed `afc_`  → DB-backed cli_credential lookup (a person).
//!   4. Else if `verifier` is configured → JWT verification path.
//!   5. Else → 401.
//!
//! The two prefixed branches sit ahead of the verifier check on purpose: both
//! are self-contained credential classes, so a deployment with no identity
//! provider configured still authenticates them rather than answering 401 to
//! a credential it could have resolved.

const std = @import("std");
const httpz = @import("httpz");

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");
const bearer = @import("bearer.zig");
const errors = @import("errors.zig");
const oidc = @import("../oidc.zig");
const scopes = @import("../scopes.zig");
const tenant_api_key_mod = @import("tenant_api_key.zig");
const cli_credential_mod = @import("cli_credential.zig");

pub const AuthCtx = auth_ctx.AuthCtx;
pub const TenantApiKey = tenant_api_key_mod.TenantApiKey;
pub const CliCredential = cli_credential_mod.CliCredential;

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
    /// Populated by MiddlewareRegistry.initChains() when the command-line
    /// credential lookup is wired. When set, any `afc_`-prefixed Bearer token
    /// is routed to the credential path, which resolves a USER principal —
    /// the difference that lets a user-scoped route refuse a tenant key.
    cli_credential: ?*CliCredential = null,

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

        if (self.cli_credential) |cli| {
            if (std.mem.startsWith(u8, provided, cli_credential_mod.CLI_CREDENTIAL_PREFIX)) {
                return cli.execute(ctx, req);
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

test {
    // Keeps every declaration analysed; behavioural coverage lives in the
    // sibling test file, which the length cap moved out of this one.
    std.testing.refAllDecls(@This());
    _ = @import("bearer_or_api_key_test.zig");
}
