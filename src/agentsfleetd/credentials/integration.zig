//! Credential-mint integration registry. A connector is a descriptor in
//! `REGISTRY`, not a branch in the broker's mint dispatch (RULE CFG).
//! M102 §1 ships the `static` integration; §2 adds `github`.
//!
//! An integration's mint receives a `MintCtx` — its effects (outbound HTTP, RS256
//! signing, the clock) and platform secrets are injected, so integrations stay
//! pure and unit-testable with no network, no DB, and a fake key.

const std = @import("std");

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

/// What an integration's mint returns. Tagged union — the broker forwards the reason.
pub const Outcome = union(enum) {
    ok: Minted,
    reconnect_required,
    mint_failed,
};

/// The broker's result. Adds `unknown_integration` (no integration for the id).
pub const MintResult = union(enum) {
    ok: Minted,
    reconnect_required,
    unknown_integration,
    mint_failed,
};

// ── Injected dependencies a mint may need ────────────────────────────────────

/// Platform-held secrets resolved daemon-side from the admin-workspace vault.
/// `static` ignores these; `github` reads its App key. An absent field means
/// that integration is unconfigured (mint fails closed, never panics).
pub const PlatformSecrets = struct {
    github: ?GithubApp = null,
};

pub const GithubApp = struct {
    app_id: []const u8,
    private_key_pem: []const u8,
};

/// An outbound HTTP request the broker performs on the integration's behalf.
pub const HttpRequest = struct {
    url: []const u8,
    bearer: []const u8,
    accept: []const u8,
    user_agent: []const u8,
    body: []const u8,
};

/// `body` is allocated with the alloc handed to `post` and owned by the caller.
pub const HttpResponse = struct {
    status: u16,
    body: []const u8,
};

/// Injected outbound-HTTP boundary (production: `std.http.Client`; tests: a fake)
/// so integrations stay std-only and unit-testable with no network.
pub const HttpExchange = struct {
    ptr: *anyopaque,
    postFn: *const fn (ptr: *anyopaque, alloc: std.mem.Allocator, req: HttpRequest) anyerror!HttpResponse,

    pub fn post(self: HttpExchange, alloc: std.mem.Allocator, req: HttpRequest) anyerror!HttpResponse {
        return self.postFn(self.ptr, alloc, req);
    }
};

/// Injected RS256 signer (production: `auth/crypto/rs256_sign`; tests: a fake).
/// Writes the signature into `out`, returns the written slice.
pub const SignFn = *const fn (out: []u8, private_key_pem: []const u8, signing_input: []const u8) anyerror![]const u8;

/// Everything an integration's mint may need; the broker builds one per call.
pub const MintCtx = struct {
    alloc: std.mem.Allocator,
    handle: std.json.Value,
    now_ms: i64,
    platform: PlatformSecrets,
    http: HttpExchange,
    sign: SignFn,
};

/// The broker's daemon-singleton dependencies, injected once and folded into
/// every `MintCtx`.
pub const Deps = struct {
    platform: PlatformSecrets,
    http: HttpExchange,
    sign: SignFn,
};

/// One registered integration: its id + how it mints from a `MintCtx`.
pub const Spec = struct {
    id: Id,
    mintFn: *const fn (ctx: MintCtx) anyerror!Outcome,
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

/// A `Deps` with no platform secrets and a boundary that refuses HTTP/signing —
/// for the daemon's static-only path (before `github` is wired) and for tests of
/// integrations that touch neither.
pub fn nullDeps() Deps {
    // SAFETY: refusePost returns error.NoHttpBoundary unconditionally and never
    // dereferences ptr, so the boundary's opaque pointer is never read.
    const refuse_http = HttpExchange{ .ptr = undefined, .postFn = refusePost };
    return .{ .platform = .{}, .http = refuse_http, .sign = refuseSign };
}

fn refusePost(ptr: *anyopaque, alloc: std.mem.Allocator, req: HttpRequest) anyerror!HttpResponse {
    _ = ptr;
    _ = alloc;
    _ = req;
    return error.NoHttpBoundary;
}

fn refuseSign(out: []u8, private_key_pem: []const u8, signing_input: []const u8) anyerror![]const u8 {
    _ = private_key_pem;
    _ = signing_input;
    return out[0..0];
}

/// `static` integration: the handle already carries the token; return it with the
/// never-expires sentinel. No upstream call (ignores http/sign/clock).
fn mintStatic(ctx: MintCtx) anyerror!Outcome {
    const obj = switch (ctx.handle) {
        .object => |o| o,
        else => return .mint_failed,
    };
    const tok_v = obj.get(FIELD_TOKEN) orelse return .reconnect_required;
    const tok = switch (tok_v) {
        .string => |s| s,
        else => return .mint_failed,
    };
    return .{ .ok = .{ .token = try ctx.alloc.dupe(u8, tok), .expires_at_ms = STATIC_NEVER_EXPIRES_MS } };
}

// ── Tests ────────────────────────────────────────────────────────────────────

/// A `MintCtx` over `handle` with the refusing boundary — for integrations that
/// touch neither http nor sign (the `static` path).
fn testCtx(alloc: std.mem.Allocator, handle: std.json.Value) MintCtx {
    const d = nullDeps();
    return .{ .alloc = alloc, .handle = handle, .now_ms = 0, .platform = d.platform, .http = d.http, .sign = d.sign };
}

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
    const outcome = try mintStatic(testCtx(alloc, parsed.value));
    try std.testing.expect(outcome == .ok);
    defer alloc.free(outcome.ok.token);
    try std.testing.expectEqualStrings("ghp_abc", outcome.ok.token);
    try std.testing.expectEqual(STATIC_NEVER_EXPIRES_MS, outcome.ok.expires_at_ms);
}

test "mintStatic: a handle missing the token field reconnects, not crashes" {
    const alloc = std.testing.allocator;
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, "{\"integration\":\"static\"}", .{});
    defer parsed.deinit();
    const outcome = try mintStatic(testCtx(alloc, parsed.value));
    try std.testing.expect(outcome == .reconnect_required);
}
