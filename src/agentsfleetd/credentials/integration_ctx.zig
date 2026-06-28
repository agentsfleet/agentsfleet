//! The injected-effects surface a mint may touch — platform secrets, the outbound
//! HTTP boundary, the RS256 signer, the clock, and the metrics hook. Split out of
//! `integration.zig` so "what a mint can reach" is separate from the registry +
//! result types, and so the broker can build these once at init (re-exported via
//! the `integration` facade for callers that want one namespace).

const std = @import("std");

/// Platform-held secrets resolved daemon-side from the admin-workspace vault.
/// `static` ignores these; `github` reads its App key. An absent field means that
/// integration is unconfigured (mint fails closed, never panics).
pub const PlatformSecrets = struct {
    github: ?GithubApp = null,
};

pub const GithubApp = struct {
    app_id: []const u8,
    private_key_pem: []const u8,
    /// The App's public GitHub handle for the connect install URL
    /// (`github.com/apps/{app_slug}/installations/new`). Non-secret; null when the
    /// vault entry omits it (connect degrades closed). NOT used by minting.
    app_slug: ?[]const u8 = null,
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

/// One mint's observable outcome, handed to the metrics hook once per call.
pub const MintEvent = struct {
    integration: []const u8,
    outcome: []const u8, // "ok" | "reconnect_required" | "mint_failed" | "unknown_integration"
    latency_ms: i64,
    cache_hit: bool,
};

/// Injected observability hook (production: real counters/logs; tests: a recorder
/// or no-op). Keeps the broker std-only while still emitting mint telemetry.
pub const Metrics = struct {
    ptr: *anyopaque,
    onMintFn: *const fn (ptr: *anyopaque, ev: MintEvent) void,

    pub fn onMint(self: Metrics, ev: MintEvent) void {
        self.onMintFn(self.ptr, ev);
    }
};

/// Everything an integration's mint may need; the broker builds one per call.
pub const MintCtx = struct {
    alloc: std.mem.Allocator,
    handle: std.json.Value,
    now_ms: i64,
    platform: PlatformSecrets,
    http: HttpExchange,
    sign: SignFn,
};

/// The broker's daemon-singleton dependencies, injected once at init and folded
/// into every `MintCtx` (plus the metrics hook the broker itself calls).
pub const Deps = struct {
    platform: PlatformSecrets,
    http: HttpExchange,
    sign: SignFn,
    metrics: Metrics,
};

/// A `Deps` with no platform secrets, a boundary that refuses HTTP/signing, and a
/// no-op metrics sink — for the daemon's static-only path (before `github` is
/// wired) and for tests of integrations that touch none of them.
pub fn nullDeps() Deps {
    // SAFETY: refusePost/refuseSign/ignoreMint never dereference their opaque ptr.
    return .{
        .platform = .{},
        .http = .{ .ptr = undefined, .postFn = refusePost },
        .sign = refuseSign,
        .metrics = .{ .ptr = undefined, .onMintFn = ignoreMint },
    };
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

fn ignoreMint(ptr: *anyopaque, ev: MintEvent) void {
    _ = ptr;
    _ = ev;
}
