//! Self-managed credential probing for tenant_provider.zig.
//!
//! Holds the ProbedSecret record, the vault probe that turns a tenant's
//! secret_ref into a validated {provider, api_key, model, base_url}
//! quad, the tenant_id → primary-workspace bridge it rides on, and the pure
//! provider⇔base_url SSRF gate the probe (and the platform-keys PUT handler)
//! share. Split out of tenant_provider_resolver.zig per RULE FLL.

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const vault = @import("vault.zig");
const logging = @import("log");
const credential_key = @import("../fleet_runtime/credential_key.zig");

const tenant_provider = @import("tenant_provider.zig");
const base_url_guard = @import("base_url_guard.zig");
const ResolveError = tenant_provider.ResolveError;

const log = logging.scoped(.secret_probe);

const S_API_KEY = "api_key";
const S_BASE_URL = "base_url";
/// Provider id in the self-managed credential JSON that opts the credential into
/// a custom OpenAI-compatible endpoint — the `base_url` field is required iff
/// the provider equals this, and forbidden otherwise (RULE UFS; the runner uses
/// the distinct `custom:<url>` wire name, never this id, when dialing nullclaw).
pub const OPENAI_COMPATIBLE_PROVIDER: []const u8 = "openai-compatible";

pub const ProbedSecret = struct {
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

/// Bridge tenant_id → primary workspace_id using the same earliest-named-
/// workspace pattern signup_bootstrap_store uses for OIDC re-bootstrap.
/// Multi-workspace tenants point self-managed credentials at the first signup-time
/// workspace; v3 may add an explicit `vault_workspace_id` column to
/// tenant_model_selection so users can pin a different workspace.
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

pub fn probeSelfManagedSecret(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    tenant_id: []const u8,
    secret_ref: []const u8,
) (ResolveError || anyerror)!ProbedSecret {
    const ws_id = try resolvePrimaryWorkspace(alloc, conn, tenant_id);
    defer alloc.free(ws_id);

    var parsed = try loadSelfManagedJson(alloc, conn, ws_id, secret_ref);
    defer parsed.deinit();

    if (parsed.value != .object) return ResolveError.SecretDataMalformed;
    const obj = parsed.value.object;
    const provider_v = obj.get("provider") orelse return ResolveError.SecretDataMalformed;
    const model_v = obj.get("model") orelse return ResolveError.SecretDataMalformed;
    if (provider_v != .string or model_v != .string) return ResolveError.SecretDataMalformed;
    if (provider_v.string.len == 0 or model_v.string.len == 0) return ResolveError.SecretDataMalformed;

    // api_key is required + non-empty for a named provider, but OPTIONAL for an
    // openai-compatible custom endpoint — a keyless gateway dials with no bearer
    // key (the spec's optional-key design). A present key must still be a string;
    // a missing or blank key on a named provider stays malformed.
    const is_compatible = std.mem.eql(u8, provider_v.string, OPENAI_COMPATIBLE_PROVIDER);
    const api_key_str: []const u8 = if (obj.get(S_API_KEY)) |kv| blk: {
        if (kv != .string) return ResolveError.SecretDataMalformed;
        break :blk kv.string;
    } else "";
    if (!is_compatible and api_key_str.len == 0) return ResolveError.SecretDataMalformed;

    // Extract the optional base_url (string when present) and validate the
    // provider⇔base_url pairing through the SSRF guard BEFORE any owned alloc, so
    // a hostile or mismatched endpoint fails the probe early. A non-string
    // base_url is malformed JSON; a validated URL is duped onto the credential.
    const base_url_opt: ?[]const u8 = if (obj.get(S_BASE_URL)) |bv| blk: {
        if (bv != .string) return ResolveError.SecretDataMalformed;
        break :blk bv.string;
    } else null;
    const validated_base_url = try validateSecretEndpoint(provider_v.string, base_url_opt);

    const provider = try alloc.dupe(u8, provider_v.string);
    errdefer alloc.free(provider);
    const api_key = try alloc.dupe(u8, api_key_str);
    errdefer {
        std.crypto.secureZero(u8, api_key);
        alloc.free(api_key);
    }
    const model = try alloc.dupe(u8, model_v.string);
    errdefer alloc.free(model);
    const base_url: ?[]u8 = if (validated_base_url) |u| try alloc.dupe(u8, u) else null;
    return .{ .provider = provider, .api_key = api_key, .model = model, .base_url = base_url };
}

/// Load the raw JSON body of a tenant-scoped secret by secret_ref, WITHOUT the
/// provider/model/api_key shape validation `probeSelfManagedSecret` enforces —
/// used by the tenant model registry (M121), whose entries own the model and
/// may reference a secret with no `model` field. Same primary-workspace bridge
/// and raw→prefixed key fallback as the probe. Returns ResolveError.SecretMissing
/// when no vault row matches either name. Caller owns the parsed value.
pub fn loadTenantSecretJson(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    tenant_id: []const u8,
    secret_ref: []const u8,
) (ResolveError || anyerror)!std.json.Parsed(std.json.Value) {
    const ws_id = try resolvePrimaryWorkspace(alloc, conn, tenant_id);
    defer alloc.free(ws_id);
    return loadSelfManagedJson(alloc, conn, ws_id, secret_ref);
}

/// Validate the provider⇔base_url pairing for a self-managed credential (RULE
/// PRI/NTP — the URL is hostile). Pure (no allocation, no DB) so the credential
/// unit tests drive every branch directly:
///   - provider == openai-compatible  ⇒ base_url REQUIRED and guard-`ok`
///     (https + SSRF-safe), else `SecretEndpointInvalid`. Returns the bare
///     validated URL (borrowed from the input) for the caller to dupe.
///   - any other provider             ⇒ base_url FORBIDDEN; present ⇒
///     `SecretEndpointInvalid`. Returns null.
/// Only the rejected host (never the api_key) is logged at the call site.
/// Pub for the co-located §6 validation unit tests (`tenant_provider_test.zig`),
/// which drive every provider⇔base_url branch without a DB.
pub fn validateSecretEndpoint(provider: []const u8, base_url_opt: ?[]const u8) ResolveError!?[]const u8 {
    const is_compatible = std.mem.eql(u8, provider, OPENAI_COMPATIBLE_PROVIDER);
    if (!is_compatible) {
        // A named provider must not smuggle a base_url (would silently widen the
        // egress allowlist without going through the compatible path).
        if (base_url_opt != null) return ResolveError.SecretEndpointInvalid;
        return null;
    }
    const base_url = base_url_opt orelse return ResolveError.SecretEndpointInvalid;
    return switch (base_url_guard.validate(base_url)) {
        .ok => base_url,
        .invalid_scheme, .blocked_host, .malformed => ResolveError.SecretEndpointInvalid,
    };
}

fn loadSelfManagedJson(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    secret_ref: []const u8,
) (ResolveError || anyerror)!std.json.Parsed(std.json.Value) {
    return vault.loadJson(alloc, conn, workspace_id, secret_ref) catch |err| switch (err) {
        error.NotFound => {
            const key_name = try credential_key.allocKeyName(alloc, secret_ref);
            defer alloc.free(key_name);
            const parsed = vault.loadJson(alloc, conn, workspace_id, key_name) catch |prefixed_err| switch (prefixed_err) {
                error.NotFound => return ResolveError.SecretMissing,
                else => return prefixed_err,
            };
            log.debug("self_managed_secret_ref_fallback", .{ .workspace_id = workspace_id });
            return parsed;
        },
        else => return err,
    };
}
