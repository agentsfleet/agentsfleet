//! `tenant_api_key` middleware.
//!
//! Resolves `Authorization: Bearer agt_t{hex}` tokens via a host-supplied
//! `LookupFn` callback. On match (and row.active = true), populates
//! `ctx.principal` with `.mode=.api_key`, `.user_id`, `.tenant_id`, and the
//! capability set the identity provider holds for the subject in `created_by`
//! — the person who minted the key. Rejects unknown keys with 401
//! ERR_UNAUTHORIZED; rejects revoked keys with 401 ERR_APIKEY_REVOKED.
//!
//! The key proves WHICH key; the provider answers WHAT it may do, per request.
//! No grant is authored here. A key is exactly as capable as the person who
//! minted it, so narrowing that person narrows every key they created without
//! a deploy and without a backfill — the same rule `cli_credential.zig`
//! already follows, applied to the last credential class that did not.
//!
//! Portability: this file MUST NOT import from `src/db/`, `src/http/`, or
//! any business-layer module (§1.2 contract; enforced by `make test-auth`).
//! The DB lookup lives behind `LookupFn`, wired by the host (serve.zig) at
//! boot.
//!
//! Lifetime: `LookupFn` returns slices owned by the caller's allocator. The
//! middleware duplicates the kept fields (`user_id`, `tenant_id`) into
//! `ctx.alloc` and frees the caller's slices before returning. The handler
//! layer then owns the principal fields, freed after the request completes.

const std = @import("std");
const httpz = @import("httpz");

const chain = @import("chain.zig");
const auth_ctx = @import("auth_ctx.zig");
const bearer = @import("bearer.zig");
const errors = @import("errors.zig");
const api_key = @import("../api_key.zig");
const scopes = @import("../scopes.zig");
const logging = @import("log");

pub const AuthCtx = auth_ctx.AuthCtx;

pub const TENANT_KEY_PREFIX = "agt_t";

const auth_codes = @import("auth_codes");
const ERR_APIKEY_REVOKED = auth_codes.ERR_APIKEY_REVOKED;

const log = logging.scoped(.api_keys);

/// Outcome of a key-hash lookup. All slices are owned by the allocator
/// passed to `LookupFn`; the caller of `LookupFn` is responsible for
/// freeing them.
const S_AUTH_REJECTED = "auth_rejected";
const S_INVALID_OR_MISSING_TOKEN = "Invalid or missing token";
const S_AUTH_UNAVAILABLE = "Authentication service unavailable";

pub const LookupResult = struct {
    api_key_id: []const u8,
    tenant_id: []const u8,
    user_id: []const u8,
    active: bool,
};

/// Host-supplied callback that resolves a SHA-256 hex digest to a key row.
/// Returns `null` when no row matches the hash. The host is responsible for
/// DB access; `src/auth/` never reaches into `src/db/`.
pub const LookupFn = *const fn (
    host: *anyopaque,
    alloc: std.mem.Allocator,
    key_hash_hex: []const u8,
) anyerror!?LookupResult;

pub const TenantApiKey = struct {
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
        const self: *TenantApiKey = @ptrCast(@alignCast(ptr));
        return execute(self, ctx, req);
    }

    pub fn execute(self: *Self, ctx: *AuthCtx, req: *httpz.Request) !chain.Outcome {
        const provided = bearer.parseBearerToken(req) orelse {
            ctx.fail(errors.ERR_UNAUTHORIZED, S_INVALID_OR_MISSING_TOKEN);
            return .short_circuit;
        };
        if (!std.mem.startsWith(u8, provided, TENANT_KEY_PREFIX)) {
            ctx.fail(errors.ERR_UNAUTHORIZED, S_INVALID_OR_MISSING_TOKEN);
            return .short_circuit;
        }
        return resolve(self, ctx, provided);
    }
};

fn resolve(self: *TenantApiKey, ctx: *AuthCtx, raw_key: []const u8) !chain.Outcome {
    const hash_hex = api_key.sha256Hex(raw_key);

    const maybe_row = self.lookup(self.host, ctx.alloc, hash_hex[0..]) catch {
        ctx.fail(errors.ERR_AUTH_UNAVAILABLE, S_AUTH_UNAVAILABLE);
        return .short_circuit;
    };
    const row = maybe_row orelse {
        log.err(S_AUTH_REJECTED, .{ .reason = "unknown", .key_prefix = TENANT_KEY_PREFIX, .error_code = errors.ERR_UNAUTHORIZED });
        ctx.fail(errors.ERR_UNAUTHORIZED, S_INVALID_OR_MISSING_TOKEN);
        return .short_circuit;
    };

    if (!row.active) {
        log.err(S_AUTH_REJECTED, .{ .reason = "revoked", .api_key_id = row.api_key_id, .error_code = ERR_APIKEY_REVOKED });
        freeRow(ctx.alloc, row);
        ctx.fail(ERR_APIKEY_REVOKED, "API key has been revoked");
        return .short_circuit;
    }

    // `row.user_id` is `created_by` — the provider's subject claim, per
    // `240_api_keys.sql`, not a `core.users` identifier. That is exactly what
    // the resolver keys on, so the subject needed here is already in hand.
    const claim = self.resolveScopes(self.scope_host, ctx.alloc, row.user_id) catch {
        log.err(S_AUTH_REJECTED, .{ .reason = "scopes_unavailable", .api_key_id = row.api_key_id, .error_code = errors.ERR_AUTH_UNAVAILABLE });
        freeRow(ctx.alloc, row);
        ctx.fail(errors.ERR_AUTH_UNAVAILABLE, S_AUTH_UNAVAILABLE);
        return .short_circuit;
    };
    defer ctx.alloc.free(claim);
    // Same parser the JWT and credential paths use, so three credential shapes
    // cannot drift in how a claim string becomes a capability set.
    const scope_set = scopes.parseClaim(claim);

    log.debug("auth_succeeded", .{ .api_key_id = row.api_key_id, .tenant_id = row.tenant_id });
    ctx.alloc.free(row.api_key_id);
    ctx.principal = .{
        .mode = .api_key,
        .user_id = row.user_id,
        .tenant_id = row.tenant_id,
        // Resolved from the provider, never granted here. A key inherits its
        // creator's set exactly — no ceiling and no subtraction, including
        // `approval_resolve` when that person holds it (Indy, Aug 13). The
        // retired `.tenant_api_key` default grant was the last place a
        // capability was authored in code for a credential that names a person.
        .scopes = scope_set,
    };
    return .next;
}

fn freeRow(alloc: std.mem.Allocator, row: LookupResult) void {
    alloc.free(row.api_key_id);
    alloc.free(row.tenant_id);
    alloc.free(row.user_id);
}
