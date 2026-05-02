//! Tenant-scoped LLM provider state — public API.
//!
//! Holds the Mode enum, platform-default constants, the ResolvedProvider
//! return shape, the ResolveError set, and the read/write entry points
//! that bridge the schema (core.tenant_providers + core.platform_llm_keys
//! + vault.secrets) into a single value the worker, doctor, and HTTP
//! handler all consume.
//!
//! Storage layout reminder. core.tenant_providers carries one row per
//! tenant who has explicitly configured a provider; absence of row is the
//! synthesised platform default. The api_key never lives in this row —
//! under platform mode the resolver follows core.platform_llm_keys into
//! the admin tenant's workspace vault; under BYOK it loads the user's
//! tenant-primary workspace vault under the user-named credential_ref.
//! The vault itself is keyed (workspace_id, key_name); the resolver
//! bridges tenant_id → primary_workspace_id at lookup time via the same
//! earliest-named-workspace pattern signup_bootstrap_store uses.
//!
//! Read-side internals (load helpers, vault probing, resolve* orchestration)
//! live in tenant_provider_resolver.zig per RULE FLL.

const std = @import("std");
const pg = @import("pg");
const resolver = @import("tenant_provider_resolver.zig");

pub const Mode = enum {
    platform,
    byok,

    pub fn label(self: Mode) []const u8 {
        return switch (self) {
            .platform => "platform",
            .byok => "byok",
        };
    }
};

/// Platform-default model resolved when a tenant has no explicit
/// tenant_providers row OR has an explicit row with mode=platform.
pub const PLATFORM_DEFAULT_MODEL: []const u8 = "accounts/fireworks/models/kimi-k2.6";

/// Platform-default context cap matching PLATFORM_DEFAULT_MODEL's row in
/// core.model_caps. Kept in sync with schema/019_model_caps.sql.
pub const PLATFORM_DEFAULT_CAP_TOKENS: u32 = 256_000;

/// Resolved provider configuration for one event. The api_key field is
/// process-internal — it never serializes into HTTP responses, logs,
/// telemetry, or doctor JSON. Callers must `deinit` to zero the api_key
/// bytes before free.
pub const ResolvedProvider = struct {
    mode: Mode,
    provider: []u8,
    /// Sensitive — bytes are zeroed by deinit before free.
    api_key: []u8,
    model: []u8,
    context_cap_tokens: u32,

    pub fn deinit(self: *ResolvedProvider, alloc: std.mem.Allocator) void {
        std.crypto.secureZero(u8, self.api_key);
        alloc.free(self.api_key);
        alloc.free(self.provider);
        alloc.free(self.model);
        self.* = undefined;
    }
};

pub const ResolveError = error{
    /// BYOK row points at a credential_ref that has no vault row.
    CredentialMissing,
    /// Vault row decrypted but the JSON object is missing required fields
    /// (provider, api_key, model).
    CredentialDataMalformed,
    /// Platform mode, but core.platform_llm_keys has no active row OR the
    /// admin workspace's vault is missing the referenced key. Operator-side
    /// incident; surfaced via dead-letter on the next event.
    PlatformKeyMissing,
    /// Tenant has no workspace at all — bootstrap invariant violated.
    /// Should never happen in practice (signup creates the primary workspace).
    TenantHasNoWorkspace,
};

// ── Public API ──────────────────────────────────────────────────────────────

/// Read tenant_providers for tenant_id and return a ResolvedProvider with
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
        return resolver.resolvePlatformDefault(alloc, conn, row);
    }
    return resolver.resolveByok(alloc, conn, tenant_id, row.?);
}

/// UPSERT a BYOK row for tenant_id. Validates the credential exists in the
/// tenant's primary workspace vault and that the JSON has the required
/// shape (provider/api_key/model). Stores the user-supplied model + cap
/// directly — caller is responsible for resolving them from the model-caps
/// catalogue beforehand.
///
/// Persisted `provider` is read from the validated credential's JSON
/// payload — not from any caller-supplied parameter — so the row reflects
/// what the resolver will actually see.
pub fn upsertByok(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    tenant_id: []const u8,
    credential_ref: []const u8,
    model: []const u8,
    context_cap_tokens: u32,
) (ResolveError || anyerror)!void {
    var probe = try resolver.probeByokCredential(alloc, conn, tenant_id, credential_ref);
    defer probe.deinit(alloc);

    const now_ms: i64 = std.time.milliTimestamp();
    _ = try conn.exec(
        \\INSERT INTO core.tenant_providers
        \\  (tenant_id, mode, provider, model, context_cap_tokens, credential_ref, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, $4, $5, $6, $7, $7)
        \\ON CONFLICT (tenant_id) DO UPDATE SET
        \\  mode               = EXCLUDED.mode,
        \\  provider           = EXCLUDED.provider,
        \\  model              = EXCLUDED.model,
        \\  context_cap_tokens = EXCLUDED.context_cap_tokens,
        \\  credential_ref     = EXCLUDED.credential_ref,
        \\  updated_at         = EXCLUDED.updated_at
    , .{
        tenant_id,
        Mode.byok.label(),
        probe.provider,
        model,
        @as(i32, @intCast(context_cap_tokens)),
        credential_ref,
        now_ms,
    });
}

/// UPSERT an explicit platform-default row for tenant_id. Used by
/// `tenant provider reset` so the dashboard can distinguish "never
/// configured" from "explicitly reset". Provider is read from the active
/// platform_llm_keys row so the row matches what resolveActiveProvider
/// will return.
pub fn upsertPlatform(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    tenant_id: []const u8,
) (ResolveError || anyerror)!void {
    var plk = try resolver.loadActivePlatformKey(alloc, conn);
    defer plk.deinit(alloc);

    const now_ms: i64 = std.time.milliTimestamp();
    _ = try conn.exec(
        \\INSERT INTO core.tenant_providers
        \\  (tenant_id, mode, provider, model, context_cap_tokens, credential_ref, created_at, updated_at)
        \\VALUES ($1::uuid, $2, $3, $4, $5, NULL, $6, $6)
        \\ON CONFLICT (tenant_id) DO UPDATE SET
        \\  mode               = EXCLUDED.mode,
        \\  provider           = EXCLUDED.provider,
        \\  model              = EXCLUDED.model,
        \\  context_cap_tokens = EXCLUDED.context_cap_tokens,
        \\  credential_ref     = NULL,
        \\  updated_at         = EXCLUDED.updated_at
    , .{
        tenant_id,
        Mode.platform.label(),
        plk.provider,
        PLATFORM_DEFAULT_MODEL,
        @as(i32, @intCast(PLATFORM_DEFAULT_CAP_TOKENS)),
        now_ms,
    });
}

/// Test-only helper: drop the tenant_providers row entirely. Production
/// code paths use upsertPlatform to reset rather than deleteRow.
pub fn deleteRow(conn: *pg.Conn, tenant_id: []const u8) !void {
    _ = try conn.exec(
        \\DELETE FROM core.tenant_providers WHERE tenant_id = $1::uuid
    , .{tenant_id});
}

test {
    _ = @import("tenant_provider_test.zig");
}
