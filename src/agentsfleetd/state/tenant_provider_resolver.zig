//! Read-side internals for tenant_provider.zig.
//!
//! Holds the ProviderRow / PlatformKey record types, the SELECT helpers that
//! load them, and the resolve* helpers that turn a tenant_model_selection row (or its
//! absence) into a fully-populated ResolvedProvider including the api_key
//! fetched from vault. The self-managed credential probe + endpoint SSRF gate
//! live in secret_probe.zig (split out per RULE FLL).

const std = @import("std");
const pg = @import("pg");
const PgQuery = @import("../db/pg_query.zig").PgQuery;
const vault = @import("vault.zig");
const logging = @import("log");
const secret_probe = @import("secret_probe.zig");

const tenant_provider = @import("tenant_provider.zig");
pub const Mode = tenant_provider.Mode;
pub const ResolvedProvider = tenant_provider.ResolvedProvider;
pub const ResolveError = tenant_provider.ResolveError;

const log = logging.scoped(.tenant_provider_resolver);

const S_API_KEY = "api_key";

pub const ProviderRow = struct {
    const Self = @This();

    mode: Mode,
    provider: []u8,
    model: []u8,
    context_cap_tokens: u32,
    secret_ref: ?[]u8,

    pub fn deinit(self: *Self, alloc: std.mem.Allocator) void {
        alloc.free(self.provider);
        alloc.free(self.model);
        if (self.secret_ref) |c| alloc.free(c);
    }
};

pub const PlatformKey = struct {
    const Self = @This();

    provider: []u8,
    source_workspace_id: []u8,
    /// The priced catalogue model this default resolves to (set by
    /// PUT /v1/admin/platform-keys, validated ∈ core.model_library at write time).
    model: []u8,
    /// Custom OpenAI-compatible endpoint for a non-named default; null for named
    /// providers (which dial a built-in host). Threaded to the runner dial.
    base_url: ?[]u8,
    context_cap_tokens: u32,

    pub fn deinit(self: *Self, alloc: std.mem.Allocator) void {
        alloc.free(self.provider);
        alloc.free(self.source_workspace_id);
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
        \\SELECT mode, provider, model, context_cap_tokens, secret_ref
        \\FROM core.tenant_model_selection
        \\WHERE tenant_id = $1::uuid
    , .{tenant_id}));
    defer q.deinit();

    const row = (try q.next()) orelse return null;
    const mode_label = try row.get([]const u8, 0);
    const mode = parseMode(mode_label) orelse {
        log.warn("bad_mode", .{ .tenant_id = tenant_id, .mode = mode_label });
        return ResolveError.SecretDataMalformed;
    };
    const provider = try alloc.dupe(u8, try row.get([]const u8, 1));
    errdefer alloc.free(provider);
    const model = try alloc.dupe(u8, try row.get([]const u8, 2));
    errdefer alloc.free(model);
    const cap_i32 = try row.get(i32, 3);
    const cred_ref_opt = try row.get(?[]const u8, 4);
    const secret_ref: ?[]u8 = if (cred_ref_opt) |c| try alloc.dupe(u8, c) else null;

    return .{
        .mode = mode,
        .provider = provider,
        .model = model,
        .context_cap_tokens = @intCast(@max(cap_i32, 0)),
        .secret_ref = secret_ref,
    };
}

fn parseMode(label: []const u8) ?Mode {
    if (std.mem.eql(u8, label, "platform")) return .platform;
    if (std.mem.eql(u8, label, "self_managed")) return .self_managed;
    return null;
}

pub fn loadActivePlatformKey(alloc: std.mem.Allocator, conn: *pg.Conn) !PlatformKey {
    // PUT /v1/admin/platform-keys enforces exactly one active row; ORDER BY
    // updated_at DESC, id DESC keeps the choice deterministic if a parallel
    // integration test seeds its own active row.
    var q = PgQuery.from(try conn.query(
        \\SELECT provider, source_workspace_id::text, model, base_url, context_cap_tokens
        \\FROM core.platform_provider_defaults
        \\WHERE active = true
        \\ORDER BY updated_at DESC, id DESC
        \\LIMIT 1
    , .{}));
    defer q.deinit();
    const row = (try q.next()) orelse return ResolveError.PlatformKeyMissing;

    // model is the priced default; a null means the active row predates a proper
    // default-set (legacy) — fail like a missing key so the operator re-sets it
    // through the dashboard rather than resolving an unpriced model.
    const model_opt = try row.get(?[]const u8, 2);
    const model_src = model_opt orelse return ResolveError.PlatformKeyMissing;
    const base_url_opt = try row.get(?[]const u8, 3);
    const cap_i32 = (try row.get(?i32, 4)) orelse 0;

    const provider = try alloc.dupe(u8, try row.get([]const u8, 0));
    errdefer alloc.free(provider);
    const ws_id = try alloc.dupe(u8, try row.get([]const u8, 1));
    errdefer alloc.free(ws_id);
    const model = try alloc.dupe(u8, model_src);
    errdefer alloc.free(model);
    const base_url: ?[]u8 = if (base_url_opt) |u| try alloc.dupe(u8, u) else null;

    return .{
        .provider = provider,
        .source_workspace_id = ws_id,
        .model = model,
        .base_url = base_url,
        .context_cap_tokens = @intCast(@max(cap_i32, 0)),
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

/// Resolve the platform default entirely from the active core.platform_provider_defaults
/// row — provider, model, context cap, and base_url all come from there, not from
/// the tenant's own tenant_model_selection snapshot or a compile-time constant. This is
/// what makes a default change propagate per-lease (next event re-resolves): an
/// admin repointing the default in the dashboard takes effect for every
/// platform-mode tenant without a redeploy or a per-tenant write.
pub fn resolvePlatformDefault(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
) (ResolveError || anyerror)!ResolvedProvider {
    var plk = try loadActivePlatformKey(alloc, conn);
    defer plk.deinit(alloc);

    const api_key = try loadVaultApiKey(alloc, conn, plk.source_workspace_id, plk.provider);
    errdefer {
        std.crypto.secureZero(u8, api_key);
        alloc.free(api_key);
    }

    const provider = try alloc.dupe(u8, plk.provider);
    errdefer alloc.free(provider);
    const model = try alloc.dupe(u8, plk.model);
    errdefer alloc.free(model);
    // Carry the admin-configured custom endpoint forward so a non-named
    // (openai-compatible) platform default actually dials it; null for named
    // providers, which dial a built-in host.
    const base_url: ?[]u8 = if (plk.base_url) |u| try alloc.dupe(u8, u) else null;

    return .{
        .mode = .platform,
        .provider = provider,
        .api_key = api_key,
        .model = model,
        .context_cap_tokens = plk.context_cap_tokens,
        .base_url = base_url,
    };
}

pub fn resolveSelfManaged(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    tenant_id: []const u8,
    row: ProviderRow,
) (ResolveError || anyerror)!ResolvedProvider {
    const secret_ref = row.secret_ref orelse return ResolveError.SecretDataMalformed;
    var cred = try secret_probe.probeSelfManagedSecret(alloc, conn, tenant_id, secret_ref);
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
