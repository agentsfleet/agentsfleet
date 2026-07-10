//! Credential-mint integration registry. A connector is a descriptor in
//! `REGISTRY`, not a branch in the broker's mint dispatch (RULE CFG).
//! M102 §1 ships the `static` integration; §2 adds `github`.
//!
//! This module owns the integration ids, the result types, the registry, and the
//! `static` integration. The injected-effects surface a mint receives lives in
//! `integration_ctx.zig` and is re-exported below so callers see one namespace.

const std = @import("std");
const common = @import("common");
const ctx = @import("integration_ctx.zig");

// ── Re-exported effect surface (defined in integration_ctx.zig) ──────────────
pub const PlatformSecrets = ctx.PlatformSecrets;
pub const GithubApp = ctx.GithubApp;
pub const OauthApp = ctx.OauthApp;
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

/// Vault-handle credential fields that rotate on an ordinary token refresh.
/// Single source for the connect callbacks (which write them), the oauth2
/// refresh mint (which reads the response twins), and the broker's cache
/// fingerprint (which EXCLUDES them: a rotation must stay a cache hit, while
/// any other field change is an identity change and must miss).
pub const FIELD_REFRESH_TOKEN: []const u8 = "refresh_token";
pub const FIELD_ACCESS_TOKEN: []const u8 = "access_token";
pub const FIELD_EXPIRES_AT_MS: []const u8 = "expires_at_ms";
pub const ROTATING_CREDENTIAL_FIELDS = [_][]const u8{ FIELD_REFRESH_TOKEN, FIELD_ACCESS_TOKEN, FIELD_EXPIRES_AT_MS };

/// Far-future sentinel for a credential with no upstream expiry (a stored PAT).
const STATIC_NEVER_EXPIRES_MS: i64 = std.math.maxInt(i64);

/// Integrations the broker can resolve. The enum field names ARE the wire values
/// stored in the vault handle (`idFromString` bridges). The three refresh-token
/// providers (zoho/jira/linear) mint via the `oauth2_refresh` strategy; the
/// api_key connectors (datadog/grafana/fly) never reach the broker — their key
/// is used directly — so they are deliberately absent here.
pub const Id = enum { static, github, zoho, jira, linear };

/// A resolved/minted credential and its validity bound (epoch ms).
pub const Minted = struct {
    token: []const u8,
    expires_at_ms: i64,
    /// Non-null only when the exchange rotated the stored refresh token
    /// (oauth2_refresh, rotating providers). Owned by the mint's allocator;
    /// the mint handler persists it back to the vault.
    rotated_refresh_token: ?[]const u8 = null,
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

/// Config for the `oauth2_refresh` strategy: the provider token endpoint (shared
/// verbatim with the connect flow via `common`, RULE UFS) plus a selector that
/// picks this provider's platform app from the injected secrets — a selector, not
/// a branch on id, so the mint stays data-driven (Invariant 4).
pub const OAuth2Refresh = struct {
    token_endpoint: []const u8,
    app: *const fn (PlatformSecrets) ?OauthApp,
};

/// How an integration resolves its credential. A tagged union over the mint
/// STRATEGY (Bun's `SideEffects` / `AllowUnresolved` idiom — data variants for the
/// common cases, a function-pointer escape hatch for the bespoke one). Dispatch is
/// the union's own `run` method, so the broker stays strategy-agnostic and adding a
/// strategy never branches the hot path (RULE CFG / Invariant 4).
///
/// `static` (handle carries a usable token, no network), `custom` (github's
/// App-JWT → installation-token exchange), and `oauth2_refresh` (a refresh-token →
/// access-token exchange at the provider token endpoint, for Zoho/Jira/Linear).
pub const Mint = union(enum) {
    /// The vault handle already carries a directly-usable stored token; resolved
    /// inline (no network). The lease path ships it as a stored value, never a
    /// mint marker — so a `static` credential never reaches the on-demand channel.
    static,
    /// Bespoke exchange — a function over the injected `MintCtx`. The escape hatch
    /// for anything the declarative strategies don't cover (github).
    custom: *const fn (mint_ctx: MintCtx) anyerror!Outcome,
    /// Refresh-token exchange: POST the handle's refresh token to the provider
    /// token endpoint for a fresh short-lived access token (Zoho/Jira/Linear).
    oauth2_refresh: OAuth2Refresh,

    /// Run this strategy against `ctx` (the union owns its dispatch — Bun's
    /// `SideEffects.hasSideEffects` idiom). The broker calls this; it never
    /// switches on integration id.
    pub fn run(self: Mint, mint_ctx: MintCtx) anyerror!Outcome {
        return switch (self) {
            .static => mintStatic(mint_ctx),
            .custom => |mintFn| mintFn(mint_ctx),
            .oauth2_refresh => |cfg| oauth_refresh.mint(mint_ctx, cfg),
        };
    }

    /// Whether this strategy defers to an on-demand broker mint (vs an inline
    /// stored value). The lease path reads this to decide marker-vs-stored-value;
    /// only `.static` is inline, so every other strategy is on-demand.
    pub fn isOnDemand(self: Mint) bool {
        return switch (self) {
            .static => false,
            .custom, .oauth2_refresh => true,
        };
    }
};

/// One registered integration: its id + how it mints.
pub const Spec = struct {
    id: Id,
    mint: Mint,
};

const oauth_refresh = @import("integration_oauth_refresh.zig");

// Platform-app selectors for the refresh-mint entries: data, not an id branch —
// each entry names its own field, so the mint never switches on provider.
fn selectZoho(p: PlatformSecrets) ?OauthApp {
    return p.zoho;
}
fn selectJira(p: PlatformSecrets) ?OauthApp {
    return p.jira;
}
fn selectLinear(p: PlatformSecrets) ?OauthApp {
    return p.linear;
}

const STATIC_SPEC = Spec{ .id = .static, .mint = .static };
const GITHUB_SPEC = Spec{ .id = .github, .mint = .{ .custom = @import("integration_github.zig").mint } };
const ZOHO_SPEC = Spec{ .id = .zoho, .mint = .{ .oauth2_refresh = .{ .token_endpoint = common.ZOHO_TOKEN_ENDPOINT, .app = selectZoho } } };
const JIRA_SPEC = Spec{ .id = .jira, .mint = .{ .oauth2_refresh = .{ .token_endpoint = common.JIRA_TOKEN_ENDPOINT, .app = selectJira } } };
const LINEAR_SPEC = Spec{ .id = .linear, .mint = .{ .oauth2_refresh = .{ .token_endpoint = common.LINEAR_TOKEN_ENDPOINT, .app = selectLinear } } };

/// All registered integrations. Adding a connector = one entry here (RULE CFG) —
/// the mint hot path never branches per id (Invariant 4).
pub const REGISTRY: []const Spec = &.{ STATIC_SPEC, GITHUB_SPEC, ZOHO_SPEC, JIRA_SPEC, LINEAR_SPEC };

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

/// The `static` integration's wire id — never a grant `service` (static is
/// resolved inline, not minted), but `toString` must be total.
const PROVIDER_STATIC: []const u8 = "static";

/// The canonical wire/DB `service` string for `id` — the value stored in
/// `core.integration_grants.service` (via the grant handler's `common.PROVIDER_*`)
/// AND emitted on `ExecutionPolicy.mintable`. Explicit, not `@tagName`, so the
/// grant gate's string comparison against the DB column has one audited source.
/// The `comptime` block below proves it round-trips through `idFromString`, so a
/// renamed enum tag or a changed `PROVIDER_*` constant is a COMPILE error — never
/// a silent grant-check miss that drops a connector's mintable without an error.
pub fn toString(id: Id) []const u8 {
    return switch (id) {
        .static => PROVIDER_STATIC,
        .github => common.PROVIDER_GITHUB,
        .zoho => common.PROVIDER_ZOHO,
        .jira => common.PROVIDER_JIRA,
        .linear => common.PROVIDER_LINEAR,
    };
}

comptime {
    for (std.enums.values(Id)) |id| {
        const s = toString(id);
        if (idFromString(s)) |round| {
            if (round != id) @compileError("integration.Id.toString/idFromString desync: " ++ @tagName(id));
        } else {
            @compileError("integration.Id.toString produced a string idFromString cannot resolve: " ++ @tagName(id));
        }
    }
}

/// Does the production registry mint `provider` via the `oauth2_refresh`
/// strategy? Comptime-usable — the connector registry (`handlers/connectors/
/// registry.zig`) calls this in a `comptime {}` block to prove every
/// refresh-capable connector has a matching broker entry, so the two registries
/// cannot silently drift (finding ①: single source of truth without a
/// cross-layer merge — the broker stays lower than the handler registry).
pub fn hasRefreshMint(provider: []const u8) bool {
    const id = idFromString(provider) orelse return false;
    for (REGISTRY) |s| {
        if (s.id == id) return s.mint == .oauth2_refresh;
    }
    return false;
}

/// Whether `id` resolves by an on-demand broker mint (delegates to the strategy's
/// own `isOnDemand` — no per-id branch, Invariant 4). An id absent from `registry`
/// is treated as not-on-demand (fail safe: a stored value, never a mint marker).
/// The lease path passes `REGISTRY`.
pub fn mintsOnDemand(registry: []const Spec, id: Id) bool {
    const s = resolve(registry, id) orelse return false;
    return s.mint.isOnDemand();
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
    // The refresh-token providers are registered alongside static/github.
    try std.testing.expectEqual(Id.zoho, resolve(REGISTRY, .zoho).?.id);
    try std.testing.expectEqual(Id.jira, resolve(REGISTRY, .jira).?.id);
    try std.testing.expectEqual(Id.linear, resolve(REGISTRY, .linear).?.id);
    // Dispatch has no implicit ids: a registry without github resolves it to null.
    const only_static: []const Spec = &.{STATIC_SPEC};
    try std.testing.expect(resolve(only_static, .github) == null);
}

test "idFromString: maps wire values, rejects unknown" {
    try std.testing.expectEqual(Id.static, idFromString("static").?);
    try std.testing.expectEqual(Id.github, idFromString("github").?);
    try std.testing.expectEqual(Id.zoho, idFromString("zoho").?);
    try std.testing.expectEqual(Id.linear, idFromString("linear").?);
    // api_key providers never reach the broker, so they are not broker ids.
    try std.testing.expect(idFromString("datadog") == null);
}

test "toString: the audited enum→service string round-trips through idFromString" {
    // pin test: these literals ARE the DB `service` column contract + the wire
    // `mintable.integration` value — the comptime block guards drift, this pins
    // the exact strings a grant row must carry.
    try std.testing.expectEqualStrings("static", toString(.static));
    try std.testing.expectEqualStrings("github", toString(.github));
    try std.testing.expectEqualStrings("zoho", toString(.zoho));
    try std.testing.expectEqualStrings("jira", toString(.jira));
    try std.testing.expectEqualStrings("linear", toString(.linear));
    inline for (std.enums.values(Id)) |id| {
        try std.testing.expectEqual(id, idFromString(toString(id)).?);
    }
}

test "hasRefreshMint: true only for oauth2_refresh providers (the ① drift guard)" {
    // The connector registry's comptime cross-check keys off this: every
    // refresh-capable connector must answer true here.
    try std.testing.expect(hasRefreshMint("zoho"));
    try std.testing.expect(hasRefreshMint("jira"));
    try std.testing.expect(hasRefreshMint("linear"));
    // github mints via `custom` (App JWT), not oauth2_refresh; static is inline.
    try std.testing.expect(!hasRefreshMint("github"));
    try std.testing.expect(!hasRefreshMint("static"));
    // Unknown / api_key ids have no broker entry at all.
    try std.testing.expect(!hasRefreshMint("datadog"));
    try std.testing.expect(!hasRefreshMint("nope"));
}

test "Mint.isOnDemand: only static resolves inline; minted strategies are on-demand" {
    // The lease path keys marker-vs-stored-value off this — a `static` handle is
    // a stored value (no mint marker), `custom` (github) + `oauth2_refresh`
    // (zoho/jira/linear) mint on demand.
    try std.testing.expect(!STATIC_SPEC.mint.isOnDemand());
    try std.testing.expect(GITHUB_SPEC.mint.isOnDemand());
    try std.testing.expect(ZOHO_SPEC.mint.isOnDemand());
    // …and routed through the registry the lease path actually calls.
    try std.testing.expect(!mintsOnDemand(REGISTRY, .static));
    try std.testing.expect(mintsOnDemand(REGISTRY, .github));
    try std.testing.expect(mintsOnDemand(REGISTRY, .jira));
}

test "Mint.run: the strategy union dispatches without a per-id branch" {
    const alloc = std.testing.allocator;
    var parsed = try std.json.parseFromSlice(std.json.Value, alloc, "{\"integration\":\"static\",\"token\":\"ghp_xyz\"}", .{});
    defer parsed.deinit();
    // .static runs the inline strategy; a `.custom` entry would call its fn — both
    // through the SAME `run`, so a new strategy never touches the broker (1.2).
    const outcome = try STATIC_SPEC.mint.run(testing.ctxOver(alloc, parsed.value));
    try std.testing.expect(outcome == .ok);
    defer alloc.free(outcome.ok.token);
    try std.testing.expectEqualStrings("ghp_xyz", outcome.ok.token);
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
