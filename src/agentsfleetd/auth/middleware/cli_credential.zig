//! `cli_credential` middleware.
//!
//! Resolves `Authorization: Bearer afc_{hex}` tokens — the durable credential
//! `agentsfleet login` mints — via a host-supplied `LookupFn`. On match, and
//! only when the row is live, populates `ctx.principal` with
//! `.mode = .cli_credential`, the creating `user_id`, and that user's
//! `tenant_id`.
//!
//! Why this is not `tenant_api_key` with a different prefix: an `agt_t` key
//! belongs to a tenant and carries the tenant grant, so a terminal holding one
//! acts as the whole organisation. This credential belongs to a person. It
//! resolves to a user principal, which is what lets a user-scoped route refuse
//! a tenant key while accepting this (Invariant 1, Dimension 1.1).
//!
//! Portability: this file MUST NOT import from `src/db/`, `src/http/`, or any
//! business-layer module — `make test-auth` enforces it. The digest → row
//! resolution lives in `cmd/cli_credential_lookup.zig`, wired by the serve host
//! at boot, exactly as the tenant-key path does.
//!
//! Lifetime: `LookupFn` returns slices owned by the allocator it was handed.
//! This middleware adopts `user_id` and `tenant_id` into the principal and
//! frees the rest before returning, so the handler layer owns exactly the two
//! fields the principal exposes.

const std = @import("std");
const httpz = @import("httpz");

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");
const bearer = @import("bearer.zig");
const errors = @import("errors.zig");
const api_key = @import("../api_key.zig");
const cli_credential = @import("../cli_credential.zig");
const scopes = @import("../scopes.zig");
const logging = @import("log");

pub const AuthCtx = auth_ctx.AuthCtx;

/// Marks a bearer value as a Command-Line Interface credential. Single-sourced
/// from the generator so the router and the minter can never disagree on shape.
pub const CLI_CREDENTIAL_PREFIX = cli_credential.PREFIX;

const auth_codes = @import("auth_codes");
const ERR_CLI_CREDENTIAL_REVOKED = auth_codes.ERR_CLI_CREDENTIAL_REVOKED;

const log = logging.scoped(.cli_credentials);

const S_AUTH_REJECTED = "auth_rejected";
const S_INVALID_OR_MISSING_TOKEN = "Invalid or missing token";
const S_REVOKED_MESSAGE = "Command-line credential has been revoked";
const S_AUTH_UNAVAILABLE = "Authentication service unavailable";

/// Outcome of a digest lookup. Every slice is owned by the allocator passed to
/// `LookupFn`; the caller frees what it does not adopt.
pub const LookupResult = struct {
    credential_id: []const u8,
    user_id: []const u8,
    tenant_id: []const u8,
    /// The deployment host recorded at mint. Carried so a future slice can
    /// refuse a credential presented to a deployment that did not issue it;
    /// §1 resolves identity only and does not read this.
    deployment: []const u8,
    /// True when `revoked_at` is set. The lookup returns the row rather than
    /// filtering it out, so a revoked credential answers its own code instead
    /// of being indistinguishable from one that never existed.
    revoked: bool,
    /// The identity provider's subject for the owning user, joined from
    /// `core.users`. This is what the scope resolver keys on: the credential
    /// proves identity, and the provider answers what that identity may do.
    oidc_subject: []const u8,
};

/// Host-supplied callback resolving a SHA-256 hex digest to a credential row.
/// Returns `null` when no row matches. `src/auth/` never reaches a datastore.
pub const LookupFn = *const fn (
    host: *anyopaque,
    alloc: std.mem.Allocator,
    credential_hash_hex: []const u8,
) anyerror!?LookupResult;

pub const CliCredential = struct {
    const Self = @This();

    host: *anyopaque,
    lookup: LookupFn,
    /// Separate host from `host`: that one owns a connection pool, this one
    /// owns a provider client and its cache. Different lifetimes, different
    /// failure modes, so they are not conflated behind one pointer.
    scope_host: *anyopaque,
    resolveScopes: scopes.ScopeFn,

    pub fn middleware(self: *Self) chain.Middleware(AuthCtx) {
        return .{ .ptr = self, .execute_fn = executeTypeErased };
    }

    fn executeTypeErased(ptr: *anyopaque, ctx: *AuthCtx, req: *httpz.Request) anyerror!chain.Outcome {
        const self: *CliCredential = @ptrCast(@alignCast(ptr));
        return execute(self, ctx, req);
    }

    pub fn execute(self: *Self, ctx: *AuthCtx, req: *httpz.Request) !chain.Outcome {
        const provided = bearer.parseBearerToken(req) orelse {
            ctx.fail(errors.ERR_UNAUTHORIZED, S_INVALID_OR_MISSING_TOKEN);
            return .short_circuit;
        };
        // Shape-check before hashing: a truncated paste or a session token
        // written into the credential slot is refused here rather than costing
        // a datastore round trip that could only answer the same thing.
        if (!cli_credential.looksWellFormed(provided)) {
            ctx.fail(errors.ERR_UNAUTHORIZED, S_INVALID_OR_MISSING_TOKEN);
            return .short_circuit;
        }
        return resolve(self, ctx, provided);
    }
};

fn resolve(self: *CliCredential, ctx: *AuthCtx, raw_credential: []const u8) !chain.Outcome {
    const hash_hex = api_key.sha256Hex(raw_credential);

    const maybe_row = self.lookup(self.host, ctx.alloc, hash_hex[0..]) catch {
        ctx.fail(errors.ERR_AUTH_UNAVAILABLE, S_AUTH_UNAVAILABLE);
        return .short_circuit;
    };
    const row = maybe_row orelse {
        log.err(S_AUTH_REJECTED, .{ .reason = "unknown", .key_prefix = CLI_CREDENTIAL_PREFIX, .error_code = errors.ERR_UNAUTHORIZED });
        ctx.fail(errors.ERR_UNAUTHORIZED, S_INVALID_OR_MISSING_TOKEN);
        return .short_circuit;
    };

    if (row.revoked) {
        log.err(S_AUTH_REJECTED, .{ .reason = "revoked", .credential_id = row.credential_id, .error_code = ERR_CLI_CREDENTIAL_REVOKED });
        freeRow(ctx.alloc, row);
        ctx.fail(ERR_CLI_CREDENTIAL_REVOKED, S_REVOKED_MESSAGE);
        return .short_circuit;
    }

    // The credential proved WHO. The provider answers WHAT, per request, so a
    // scope edit reaches a terminal the way it already reaches the dashboard —
    // no grant is authored here and nothing about capabilities is stored.
    const claim = self.resolveScopes(self.scope_host, ctx.alloc, row.oidc_subject) catch {
        log.err(S_AUTH_REJECTED, .{ .reason = "scopes_unavailable", .credential_id = row.credential_id, .error_code = errors.ERR_AUTH_UNAVAILABLE });
        freeRow(ctx.alloc, row);
        ctx.fail(errors.ERR_AUTH_UNAVAILABLE, S_AUTH_UNAVAILABLE);
        return .short_circuit;
    };
    defer ctx.alloc.free(claim);
    // Same parser the JWT path uses, so the two credential shapes cannot drift
    // in how a claim string becomes a capability set.
    const scope_set = scopes.parseClaim(claim);

    log.debug("auth_succeeded", .{ .credential_id = row.credential_id, .user_id = row.user_id });
    // The principal adopts the SUBJECT as `user_id`, not the `core.users` row
    // id, because that is what the field means everywhere else: the JWT path
    // puts the token's `sub` there, and `240` records `created_by` as "the
    // identity provider's subject claim". Downstream code resolves a user by
    // `oidc_subject` (common_authz.resolvePrincipalTenant), so handing it a
    // UUID would silently resolve nothing.
    ctx.alloc.free(row.credential_id);
    ctx.alloc.free(row.deployment);
    ctx.alloc.free(row.user_id);
    ctx.principal = .{
        .mode = .cli_credential,
        .user_id = row.oidc_subject,
        .tenant_id = row.tenant_id,
        // Resolved above from the identity provider, never granted here. This
        // is the one credential source whose capabilities are not authored in
        // code (`scopes.zig::DefaultGrant`), because it is the one credential
        // that IS a person: a scope edit must reach a terminal the same way it
        // reaches the dashboard, and a collaborator provisioned narrowly must
        // not be widened by running `login`.
        .scopes = scope_set,
    };
    return .next;
}

fn freeRow(alloc: std.mem.Allocator, row: LookupResult) void {
    alloc.free(row.credential_id);
    alloc.free(row.user_id);
    alloc.free(row.tenant_id);
    alloc.free(row.deployment);
    alloc.free(row.oidc_subject);
}

test {
    // Keeps every declaration above analysed even though nothing in the tree
    // calls this module yet. Behavioural coverage lives in the sibling
    // `cli_credential_test.zig`; this only guarantees the bodies compile.
    std.testing.refAllDecls(@This());
}
