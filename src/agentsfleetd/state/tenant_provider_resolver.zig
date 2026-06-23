//! Read-side internals for tenant_provider.zig.
//!
//! Holds the ProviderRow / PlatformKey / ProbedCredential record types,
//! the SELECT helpers that load them, the bridge from tenant_id to the
//! tenant's primary workspace_id, and the resolve* helpers that turn a
//! tenant_providers row (or its absence) into a fully-populated
//! ResolvedProvider including the api_key fetched from vault.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const vault = @import("vault.zig");
const logging = @import("log");
const credential_key = @import("../fleet_runtime/credential_key.zig");

const tenant_provider = @import("tenant_provider.zig");
const base_url_guard = @import("base_url_guard.zig");
pub const Mode = tenant_provider.Mode;
pub const ResolvedProvider = tenant_provider.ResolvedProvider;
pub const ResolveError = tenant_provider.ResolveError;
pub const PLATFORM_DEFAULT_MODEL = tenant_provider.PLATFORM_DEFAULT_MODEL;
pub const PLATFORM_DEFAULT_CAP_TOKENS = tenant_provider.PLATFORM_DEFAULT_CAP_TOKENS;

const log = logging.scoped(.tenant_provider_resolver);

const S_API_KEY = "api_key";
const S_BASE_URL = "base_url";
/// Provider id in the self-managed credential JSON that opts the credential into
/// a custom OpenAI-compatible endpoint — the `base_url` field is required iff
/// the provider equals this, and forbidden otherwise (RULE UFS; the runner uses
/// the distinct `custom:<url>` wire name, never this id, when dialing nullclaw).
pub const OPENAI_COMPATIBLE_PROVIDER: []const u8 = "openai-compatible";

pub const ProviderRow = struct {
    const Self = @This();

    mode: Mode,
    provider: []u8,
    model: []u8,
    context_cap_tokens: u32,
    credential_ref: ?[]u8,

    pub fn deinit(self: *Self, alloc: std.mem.Allocator) void {
        alloc.free(self.provider);
        alloc.free(self.model);
        if (self.credential_ref) |c| alloc.free(c);
    }
};

pub const PlatformKey = struct {
    const Self = @This();

    provider: []u8,
    source_workspace_id: []u8,

    pub fn deinit(self: *Self, alloc: std.mem.Allocator) void {
        alloc.free(self.provider);
        alloc.free(self.source_workspace_id);
    }
};

pub const ProbedCredential = struct {
    const Self = @This();

    provider: []u8,
    api_key: []u8,
    model: []u8,
    /// Validated custom endpoint URL when `provider == OPENAI_COMPATIBLE_PROVIDER`
    /// (https + SSRF-safe); `null` for every named provider. Set only after
    /// base_url_guard accepts it, so a probe that succeeds carries a safe URL.
    base_url: ?[]u8 = null,

    pub fn deinit(self: *Self, alloc: std.mem.Allocator) void {
        std.crypto.secureZero(u8, self.api_key);
        alloc.free(self.api_key);
        alloc.free(self.provider);
        alloc.free(self.model);
        if (self.base_url) |u| alloc.free(u);
    }
};

pub fn loadProviderRow(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    tenant_id: []const u8,
) !?ProviderRow {
    var q = PgQuery.from(try conn.query(
        \\SELECT mode, provider, model, context_cap_tokens, credential_ref
        \\FROM core.tenant_providers
        \\WHERE tenant_id = $1::uuid
    , .{tenant_id}));
    defer q.deinit();

    const row = (try q.next()) orelse return null;
    const mode_label = try row.get([]const u8, 0);
    const mode = parseMode(mode_label) orelse {
        log.warn("bad_mode", .{ .tenant_id = tenant_id, .mode = mode_label });
        return ResolveError.CredentialDataMalformed;
    };
    const provider = try alloc.dupe(u8, try row.get([]const u8, 1));
    errdefer alloc.free(provider);
    const model = try alloc.dupe(u8, try row.get([]const u8, 2));
    errdefer alloc.free(model);
    const cap_i32 = try row.get(i32, 3);
    const cred_ref_opt = try row.get(?[]const u8, 4);
    const credential_ref: ?[]u8 = if (cred_ref_opt) |c| try alloc.dupe(u8, c) else null;

    return .{
        .mode = mode,
        .provider = provider,
        .model = model,
        .context_cap_tokens = @intCast(@max(cap_i32, 0)),
        .credential_ref = credential_ref,
    };
}

fn parseMode(label: []const u8) ?Mode {
    if (std.mem.eql(u8, label, "platform")) return .platform;
    if (std.mem.eql(u8, label, "self_managed")) return .self_managed;
    return null;
}

pub fn loadActivePlatformKey(alloc: std.mem.Allocator, conn: *pg.Conn) !PlatformKey {
    // ORDER BY updated_at DESC, id DESC: deterministic when more than one
    // active row exists. Production runs with exactly one active row per
    // the v2.0 spec; the ordering protects integration test isolation
    // when sibling tests seed their own active rows in parallel.
    var q = PgQuery.from(try conn.query(
        \\SELECT provider, source_workspace_id::text
        \\FROM core.platform_llm_keys
        \\WHERE active = true
        \\ORDER BY updated_at DESC, id DESC
        \\LIMIT 1
    , .{}));
    defer q.deinit();
    const row = (try q.next()) orelse return ResolveError.PlatformKeyMissing;
    const provider = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(provider);
    const ws_id = try alloc.dupe(u8, try row.get([]const u8, 1));
    return .{ .provider = provider, .source_workspace_id = ws_id };
}

/// Bridge tenant_id → primary workspace_id using the same earliest-named-
/// workspace pattern signup_bootstrap_store uses for OIDC re-bootstrap.
/// Multi-workspace tenants point self-managed credentials at the first signup-time
/// workspace; v3 may add an explicit `vault_workspace_id` column to
/// tenant_providers so users can pin a different workspace.
fn resolvePrimaryWorkspace(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    tenant_id: []const u8,
) ![]u8 {
    var q = PgQuery.from(try conn.query(
        \\SELECT workspace_id::text
        \\FROM core.workspaces
        \\WHERE tenant_id = $1::uuid
        \\ORDER BY created_at ASC, workspace_id ASC
        \\LIMIT 1
    , .{tenant_id}));
    defer q.deinit();
    const row = (try q.next()) orelse return ResolveError.TenantHasNoWorkspace;
    return alloc.dupe(u8, try row.get([]const u8, 0));
}

pub fn probeSelfManagedCredential(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    tenant_id: []const u8,
    credential_ref: []const u8,
) (ResolveError || anyerror)!ProbedCredential {
    const ws_id = try resolvePrimaryWorkspace(alloc, conn, tenant_id);
    defer alloc.free(ws_id);

    var parsed = try loadSelfManagedJson(alloc, conn, ws_id, credential_ref);
    defer parsed.deinit();

    if (parsed.value != .object) return ResolveError.CredentialDataMalformed;
    const obj = parsed.value.object;
    const provider_v = obj.get("provider") orelse return ResolveError.CredentialDataMalformed;
    const api_key_v = obj.get(S_API_KEY) orelse return ResolveError.CredentialDataMalformed;
    const model_v = obj.get("model") orelse return ResolveError.CredentialDataMalformed;
    if (provider_v != .string or api_key_v != .string or model_v != .string) return ResolveError.CredentialDataMalformed;
    if (provider_v.string.len == 0 or api_key_v.string.len == 0 or model_v.string.len == 0) return ResolveError.CredentialDataMalformed;

    // Extract the optional base_url (string when present) and validate the
    // provider⇔base_url pairing through the SSRF guard BEFORE any owned alloc, so
    // a hostile or mismatched endpoint fails the probe early. A non-string
    // base_url is malformed JSON; a validated URL is duped onto the credential.
    const base_url_opt: ?[]const u8 = if (obj.get(S_BASE_URL)) |bv| blk: {
        if (bv != .string) return ResolveError.CredentialDataMalformed;
        break :blk bv.string;
    } else null;
    const validated_base_url = try validateCredentialEndpoint(provider_v.string, base_url_opt);

    const provider = try alloc.dupe(u8, provider_v.string);
    errdefer alloc.free(provider);
    const api_key = try alloc.dupe(u8, api_key_v.string);
    errdefer {
        std.crypto.secureZero(u8, api_key);
        alloc.free(api_key);
    }
    const model = try alloc.dupe(u8, model_v.string);
    errdefer alloc.free(model);
    const base_url: ?[]u8 = if (validated_base_url) |u| try alloc.dupe(u8, u) else null;
    return .{ .provider = provider, .api_key = api_key, .model = model, .base_url = base_url };
}

/// Validate the provider⇔base_url pairing for a self-managed credential (RULE
/// PRI/NTP — the URL is hostile). Pure (no allocation, no DB) so the credential
/// unit tests drive every branch directly:
///   - provider == openai-compatible  ⇒ base_url REQUIRED and guard-`ok`
///     (https + SSRF-safe), else `CredentialEndpointInvalid`. Returns the bare
///     validated URL (borrowed from the input) for the caller to dupe.
///   - any other provider             ⇒ base_url FORBIDDEN; present ⇒
///     `CredentialEndpointInvalid`. Returns null.
/// Only the rejected host (never the api_key) is logged at the call site.
/// Pub for the co-located §6 validation unit tests (`tenant_provider_test.zig`),
/// which drive every provider⇔base_url branch without a DB.
pub fn validateCredentialEndpoint(provider: []const u8, base_url_opt: ?[]const u8) ResolveError!?[]const u8 {
    const is_compatible = std.mem.eql(u8, provider, OPENAI_COMPATIBLE_PROVIDER);
    if (!is_compatible) {
        // A named provider must not smuggle a base_url (would silently widen the
        // egress allowlist without going through the compatible path).
        if (base_url_opt != null) return ResolveError.CredentialEndpointInvalid;
        return null;
    }
    const base_url = base_url_opt orelse return ResolveError.CredentialEndpointInvalid;
    return switch (base_url_guard.validate(base_url)) {
        .ok => base_url,
        .invalid_scheme, .blocked_host, .malformed => ResolveError.CredentialEndpointInvalid,
    };
}

fn loadSelfManagedJson(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    credential_ref: []const u8,
) (ResolveError || anyerror)!std.json.Parsed(std.json.Value) {
    return vault.loadJson(alloc, conn, workspace_id, credential_ref) catch |err| switch (err) {
        error.NotFound => {
            const key_name = try credential_key.allocKeyName(alloc, credential_ref);
            defer alloc.free(key_name);
            const parsed = vault.loadJson(alloc, conn, workspace_id, key_name) catch |prefixed_err| switch (prefixed_err) {
                error.NotFound => return ResolveError.CredentialMissing,
                else => return prefixed_err,
            };
            log.debug("self_managed_credential_ref_fallback", .{ .workspace_id = workspace_id });
            return parsed;
        },
        else => return err,
    };
}

fn loadVaultApiKey(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
) ![]u8 {
    var parsed = vault.loadJson(alloc, conn, workspace_id, key_name) catch |err| switch (err) {
        error.NotFound => return ResolveError.PlatformKeyMissing,
        else => return err,
    };
    defer parsed.deinit();

    if (parsed.value != .object) return ResolveError.PlatformKeyMissing;
    const api_key_v = parsed.value.object.get(S_API_KEY) orelse return ResolveError.PlatformKeyMissing;
    if (api_key_v != .string or api_key_v.string.len == 0) return ResolveError.PlatformKeyMissing;
    return alloc.dupe(u8, api_key_v.string);
}

pub fn resolvePlatformDefault(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    row_opt: ?ProviderRow,
) (ResolveError || anyerror)!ResolvedProvider {
    var plk = try loadActivePlatformKey(alloc, conn);
    defer plk.deinit(alloc);

    const api_key = try loadVaultApiKey(alloc, conn, plk.source_workspace_id, plk.provider);
    errdefer {
        std.crypto.secureZero(u8, api_key);
        alloc.free(api_key);
    }

    const provider_src: []const u8 = if (row_opt) |r| r.provider else plk.provider;
    const model_src: []const u8 = if (row_opt) |r| r.model else PLATFORM_DEFAULT_MODEL;
    const cap_src: u32 = if (row_opt) |r| r.context_cap_tokens else PLATFORM_DEFAULT_CAP_TOKENS;

    const provider = try alloc.dupe(u8, provider_src);
    errdefer alloc.free(provider);
    const model = try alloc.dupe(u8, model_src);
    errdefer alloc.free(model);

    return .{
        .mode = .platform,
        .provider = provider,
        .api_key = api_key,
        .model = model,
        .context_cap_tokens = cap_src,
    };
}

pub fn resolveSelfManaged(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    tenant_id: []const u8,
    row: ProviderRow,
) (ResolveError || anyerror)!ResolvedProvider {
    const credential_ref = row.credential_ref orelse return ResolveError.CredentialDataMalformed;
    var cred = try probeSelfManagedCredential(alloc, conn, tenant_id, credential_ref);
    defer cred.deinit(alloc);

    const provider = try alloc.dupe(u8, cred.provider);
    errdefer alloc.free(provider);
    const api_key = try alloc.dupe(u8, cred.api_key);
    errdefer {
        std.crypto.secureZero(u8, api_key);
        alloc.free(api_key);
    }
    const model = try alloc.dupe(u8, row.model);
    errdefer alloc.free(model);
    // Carry the guard-validated custom endpoint forward (already https + SSRF-safe
    // from the probe). `cred` owns its copy; dupe onto the resolved value so it
    // survives `cred.deinit`. Null for every named provider.
    const base_url: ?[]u8 = if (cred.base_url) |u| try alloc.dupe(u8, u) else null;

    return .{
        .mode = .self_managed,
        .provider = provider,
        .api_key = api_key,
        .model = model,
        .context_cap_tokens = row.context_cap_tokens,
        .base_url = base_url,
    };
}
