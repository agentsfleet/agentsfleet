//! Tenant-scoped LLM provider state — public API.
//!
//! Holds the Mode enum, platform-default constants, the ResolvedProvider
//! return shape, the ResolveError set, and the read/write entry points
//! that bridge the schema (core.tenant_model_selection + core.platform_provider_defaults
//! + vault.secrets) into a single value the worker, doctor, and HTTP
//! handler all consume.
//!
//! Storage layout reminder. core.tenant_model_selection carries one row per
//! tenant who has explicitly configured a provider; absence of row is the
//! synthesised platform default. The api_key never lives in this row —
//! under platform mode the resolver follows core.platform_provider_defaults into
//! the admin tenant's workspace vault; under self-managed it loads the user's
//! tenant-primary workspace vault by first trying the raw user-named
//! secret_ref, then the dashboard workspace credential key derived from it.
//! The vault itself is keyed (workspace_id, key_name); the resolver
//! bridges tenant_id → primary_workspace_id at lookup time via the same
//! earliest-named-workspace pattern signup_bootstrap_store uses.
//!
//! Read-side internals (load helpers, vault probing, resolve* orchestration)
//! live in tenant_provider_resolver.zig per RULE FLL.

const std = @import("std");
const clock = @import("common").clock;
const pg = @import("pg");
const resolver = @import("tenant_provider_resolver.zig");
const secret_probe = @import("secret_probe.zig");
const sql = @import("tenant_provider/sql.zig");

pub const Mode = enum {
    const Self = @This();

    platform,
    self_managed,

    pub fn label(self: Self) []const u8 {
        return switch (self) {
            .platform => "platform",
            .self_managed => "self_managed",
        };
    }
};

/// Resolved provider configuration for one event. The api_key field is
/// process-internal — it never serializes into HTTP responses, logs,
/// telemetry, or doctor JSON. Callers must `deinit` to zero the api_key
/// bytes before free.
pub const ResolvedProvider = struct {
    const Self = @This();

    mode: Mode,
    provider: []u8,
    /// Sensitive — bytes are zeroed by deinit before free.
    api_key: []u8,
    model: []u8,
    context_cap_tokens: u32,
    /// Validated custom endpoint URL for an `openai-compatible` self-managed
    /// credential (https + SSRF-safe, checked by base_url_guard at resolve time);
    /// `null` for every named-provider / platform credential. The control plane
    /// derives the egress-allowlist host from this and hands the engine the
    /// `custom:<url>` provider name so the request dials exactly this host.
    base_url: ?[]u8 = null,

    pub fn deinit(self: *Self, alloc: std.mem.Allocator) void {
        std.crypto.secureZero(u8, self.api_key);
        alloc.free(self.api_key);
        alloc.free(self.provider);
        alloc.free(self.model);
        if (self.base_url) |u| alloc.free(u);
        self.* = undefined;
    }
};

pub const ResolveError = error{
    /// self-managed row points at a secret_ref that has no vault row.
    SecretMissing,
    /// Vault row decrypted but the JSON object is missing required fields
    /// (provider, api_key, model).
    SecretDataMalformed,
    /// An openai-compatible credential's `base_url` is missing, not https, or
    /// targets an SSRF-unsafe host; OR a non-openai-compatible credential
    /// carries a `base_url`. Validated at the parse boundary by base_url_guard
    /// (Invariant 5) — never dialed.
    SecretEndpointInvalid,
    /// Platform mode, but core.platform_provider_defaults has no active row OR the
    /// admin workspace's vault is missing the referenced key. Operator-side
    /// incident; surfaced via dead-letter on the next event.
    PlatformKeyMissing,
    /// Tenant has no workspace at all — bootstrap invariant violated.
    /// Should never happen in practice (signup creates the primary workspace).
    TenantHasNoWorkspace,
};

// ── Public API ──────────────────────────────────────────────────────────────

/// Read tenant_model_selection for tenant_id and return a ResolvedProvider with
/// the api_key fetched from the appropriate vault row. Caller owns the
/// returned struct and must call .deinit(alloc).
pub fn resolveActiveProvider(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    tenant_id: []const u8,
) (ResolveError || anyerror)!ResolvedProvider {
    const row = try resolver.loadProviderRow(alloc, conn, tenant_id);
    defer if (row) |*r| @constCast(r).deinit(alloc);

    if (row == null or row.?.mode == .platform) {
        // The platform default is sourced wholly from the active platform key row
        // (live, per-lease) — the tenant's own snapshot is intentionally ignored.
        return resolver.resolvePlatformDefault(alloc, conn);
    }
    return resolver.resolveSelfManaged(alloc, conn, tenant_id, row.?);
}

/// UPSERT a self-managed row for tenant_id. Validates the credential exists in the
/// tenant's primary workspace vault and that the JSON has the required
/// shape (provider/api_key/model). Stores the user-supplied model + cap
/// directly — caller is responsible for resolving them from the model-caps
/// catalogue beforehand.
///
/// Persisted `provider` is read from the validated credential's JSON
/// payload — not from any caller-supplied parameter — so the row reflects
/// what the resolver will actually see.
pub fn upsertSelfManaged(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    tenant_id: []const u8,
    secret_ref: []const u8,
    model: []const u8,
    context_cap_tokens: u32,
) (ResolveError || anyerror)!void {
    var probe = try secret_probe.probeSelfManagedSecret(alloc, conn, tenant_id, secret_ref);
    defer probe.deinit(alloc);

    const now_ms: i64 = clock.nowMillis();
    _ = try conn.exec(sql.UPSERT_SELF_MANAGED, .{
        tenant_id,
        Mode.self_managed.label(),
        probe.provider,
        model,
        @as(i32, @intCast(context_cap_tokens)),
        secret_ref,
        now_ms,
    });
}

/// UPSERT an explicit platform-default row for tenant_id. Used by
/// `tenant provider reset` so the dashboard can distinguish "never
/// configured" from "explicitly reset". Provider is read from the active
/// platform_provider_defaults row so the row matches what resolveActiveProvider
/// will return.
pub fn upsertPlatform(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    tenant_id: []const u8,
) (ResolveError || anyerror)!void {
    var plk = try resolver.loadActivePlatformKey(alloc, conn);
    defer plk.deinit(alloc);

    const now_ms: i64 = clock.nowMillis();
    _ = try conn.exec(sql.UPSERT_PLATFORM, .{
        tenant_id,
        Mode.platform.label(),
        plk.provider,
        plk.model,
        @as(i32, @intCast(plk.context_cap_tokens)),
        now_ms,
    });
}

/// The active platform default's display fields (provider/model/cap) — no key,
/// no vault touch. For the GET /v1/tenants/me/provider view a tenant with no
/// explicit row falls back to. `null` when no platform default is configured.
/// Caller owns the returned strings and must call .deinit(alloc).
pub const PlatformDefaultView = struct {
    const Self = @This();

    provider: []u8,
    model: []u8,
    context_cap_tokens: u32,

    pub fn deinit(self: *Self, alloc: std.mem.Allocator) void {
        alloc.free(self.provider);
        alloc.free(self.model);
    }
};

pub fn platformDefaultView(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
) (ResolveError || anyerror)!?PlatformDefaultView {
    var plk = resolver.loadActivePlatformKey(alloc, conn) catch |err| switch (err) {
        ResolveError.PlatformKeyMissing => return null,
        else => return err,
    };
    defer plk.deinit(alloc);

    const provider = try alloc.dupe(u8, plk.provider);
    errdefer alloc.free(provider);
    const model = try alloc.dupe(u8, plk.model);
    return .{ .provider = provider, .model = model, .context_cap_tokens = plk.context_cap_tokens };
}

pub const ProbedSecret = secret_probe.ProbedSecret;

/// The provider id that opts a self-managed credential into a custom
/// OpenAI-compatible endpoint (re-exported from secret_probe — the
/// credential JSON's `provider` value, distinct from the runner's `custom:<url>`
/// wire name).
pub const OPENAI_COMPATIBLE_PROVIDER = secret_probe.OPENAI_COMPATIBLE_PROVIDER;

/// Validate a self-managed credential's provider⇔base_url pairing (pure; SSRF +
/// https-checked). Re-exported for the §6 validation unit tests.
pub const validateSecretEndpoint = secret_probe.validateSecretEndpoint;

/// Probe the tenant's self-managed credential and return the {provider, api_key,
/// model} triplet. Used by the HTTP PUT handler to read the effective
/// model from the credential before catalogue validation, and by tests.
/// Caller owns the returned struct and must call .deinit(alloc) — the
/// api_key bytes are zeroed on free.
pub fn probeSelfManaged(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    tenant_id: []const u8,
    secret_ref: []const u8,
) (ResolveError || anyerror)!ProbedSecret {
    return secret_probe.probeSelfManagedSecret(alloc, conn, tenant_id, secret_ref);
}

test {
    _ = @import("tenant_provider_test.zig");
    _ = @import("tenant_provider_endpoint_test.zig");
    _ = @import("tenant_provider_upsert_test.zig");
    _ = @import("base_url_guard.zig");
}
