//! Credential-mint integration registry. A connector is a descriptor in
//! `REGISTRY`, not a branch in the broker's mint dispatch (RULE CFG).
//! M102 §1 ships the `static` integration; §2 adds `github`.
//!
//! This module owns the integration ids, the result types, the registry, and the
//! `static` integration. The injected-effects surface a mint receives lives in
//! `integration_ctx.zig` and is re-exported below so callers see one namespace.

const std = @import("std");
const ctx = @import("integration_ctx.zig");

// ── Re-exported effect surface (defined in integration_ctx.zig) ──────────────
pub const PlatformSecrets = ctx.PlatformSecrets;
pub const GithubApp = ctx.GithubApp;
pub const HttpRequest = ctx.HttpRequest;
pub const HttpResponse = ctx.HttpResponse;
pub const HttpExchange = ctx.HttpExchange;
pub const SignFn = ctx.SignFn;
pub const Metrics = ctx.Metrics;
pub const MintEvent = ctx.MintEvent;
pub const MintCtx = ctx.MintCtx;
pub const Deps = ctx.Deps;
pub const nullDeps = ctx.nullDeps;

/// Vault-handle field carrying the integration id. Shared with the broker.
pub const FIELD_INTEGRATION: []const u8 = "integration";
/// Vault-handle field carrying a stored token (the `static` integration).
const FIELD_TOKEN: []const u8 = "token";

/// Far-future sentinel for a credential with no upstream expiry (a stored PAT).
const STATIC_NEVER_EXPIRES_MS: i64 = std.math.maxInt(i64);

/// Integrations the broker can resolve. The enum field names ARE the wire values
/// stored in the vault handle (`idFromString` bridges).
pub const Id = enum { static, github };

/// A resolved/minted credential and its validity bound (epoch ms).
pub const Minted = struct {
    token: []const u8,
    expires_at_ms: i64,
};

/// Whether a `mint_failed` is worth retrying (ECL): `transient` for upstream 5xx /
/// network / timeout; `permanent` for a malformed response or a misconfiguration.
pub const Retry = enum { transient, permanent };

/// What an integration's mint returns. Tagged union — the broker forwards the reason.
pub const Outcome = union(enum) {
    ok: Minted,
    reconnect_required,
    mint_failed: Retry,
};

/// The broker's result. Adds `unknown_integration` (no integration for the id).
pub const MintResult = union(enum) {
    ok: Minted,
    reconnect_required,
    unknown_integration,
    mint_failed: Retry,
};

/// One registered integration: its id + how it mints from a `MintCtx`.
pub const Spec = struct {
    id: Id,
    mintFn: *const fn (mint_ctx: MintCtx) anyerror!Outcome,
};

const STATIC_SPEC = Spec{ .id = .static, .mintFn = mintStatic };
const GITHUB_SPEC = Spec{ .id = .github, .mintFn = @import("integration_github.zig").mint };

/// All registered integrations. Adding a connector = one entry here (RULE CFG) —
/// the mint hot path never branches per id (Invariant 4).
pub const REGISTRY: []const Spec = &.{ STATIC_SPEC, GITHUB_SPEC };

comptime {
    for (REGISTRY, 0..) |a, i| {
        for (REGISTRY[i + 1 ..]) |b| {
            if (a.id == b.id) @compileError("duplicate Id in REGISTRY");
        }
    }
}

/// Resolve an id to its integration in `registry` (injected so tests pass a fake).
/// No per-id branch — dispatch is data (Invariant 4).
pub fn resolve(registry: []const Spec, id: Id) ?*const Spec {
    for (registry) |*s| {
        if (s.id == id) return s;
    }
    return null;
}

/// Map the vault `integration` string to an `Id`; unknown → null.
pub fn idFromString(s: []const u8) ?Id {
    return std.meta.stringToEnum(Id, s);
}

/// `static` integration: the handle already carries the token; return it with the
/// never-expires sentinel. No upstream call (ignores http/sign/clock).
fn mintStatic(mint_ctx: MintCtx) anyerror!Outcome {
    const obj = switch (mint_ctx.handle) {
        .object => |o| o,
        else => return .{ .mint_failed = .permanent },
    };
    const tok_v = obj.get(FIELD_TOKEN) orelse return .reconnect_required;
    const tok = switch (tok_v) {
        .string => |s| s,
        else => return .{ .mint_failed = .permanent },
    };
    return .{ .ok = .{ .token = try mint_ctx.alloc.dupe(u8, tok), .expires_at_ms = STATIC_NEVER_EXPIRES_MS } };
}

// ── Tests ────────────────────────────────────────────────────────────────────

const testing = @import("testing.zig");

test "resolve: finds every registered integration; a registry that omits an id returns null" {
    try std.testing.expectEqual(Id.static, resolve(REGISTRY, .static).?.id);
    try std.testing.expectEqual(Id.github, resolve(REGISTRY, .github).?.id);
    // Dispatch has no implicit ids: a registry without github resolves it to null.
    const only_static: []const Spec = &.{STATIC_SPEC};
    try std.testing.expect(resolve(only_static, .github) == null);
}

test "idFromString: maps wire values, rejects unknown" {
    try std.testing.expectEqual(Id.static, idFromString("static").?);
    try std.testing.expectEqual(Id.github, idFromString("github").?);
    try std.testing.expect(idFromString("zoho") == null);
}

test "mintStatic: returns the stored token with the never-expires bound" {
    const alloc = std.testing.allocator;
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, "{\"integration\":\"static\",\"token\":\"ghp_abc\"}", .{});
    defer parsed.deinit();
    const outcome = try mintStatic(testing.ctxOver(alloc, parsed.value));
    try std.testing.expect(outcome == .ok);
    defer alloc.free(outcome.ok.token);
    try std.testing.expectEqualStrings("ghp_abc", outcome.ok.token);
    try std.testing.expectEqual(STATIC_NEVER_EXPIRES_MS, outcome.ok.expires_at_ms);
}

test "mintStatic: a handle missing the token field reconnects, not crashes" {
    const alloc = std.testing.allocator;
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, "{\"integration\":\"static\"}", .{});
    defer parsed.deinit();
    const outcome = try mintStatic(testing.ctxOver(alloc, parsed.value));
    try std.testing.expect(outcome == .reconnect_required);
}
