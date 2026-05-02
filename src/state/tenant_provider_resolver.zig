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

pub const Mode = @import("tenant_provider.zig").Mode;
pub const ResolvedProvider = @import("tenant_provider.zig").ResolvedProvider;
pub const ResolveError = @import("tenant_provider.zig").ResolveError;
pub const PLATFORM_DEFAULT_MODEL = @import("tenant_provider.zig").PLATFORM_DEFAULT_MODEL;
pub const PLATFORM_DEFAULT_CAP_TOKENS = @import("tenant_provider.zig").PLATFORM_DEFAULT_CAP_TOKENS;

const log = std.log.scoped(.tenant_provider_resolver);

pub const ProviderRow = struct {
    mode: Mode,
    provider: []u8,
    model: []u8,
    context_cap_tokens: u32,
    credential_ref: ?[]u8,

    pub fn deinit(self: *ProviderRow, alloc: std.mem.Allocator) void {
        alloc.free(self.provider);
        alloc.free(self.model);
        if (self.credential_ref) |c| alloc.free(c);
    }
};

pub const PlatformKey = struct {
    provider: []u8,
    source_workspace_id: []u8,

    pub fn deinit(self: *PlatformKey, alloc: std.mem.Allocator) void {
        alloc.free(self.provider);
        alloc.free(self.source_workspace_id);
    }
};

pub const ProbedCredential = struct {
    provider: []u8,
    api_key: []u8,

    pub fn deinit(self: *ProbedCredential, alloc: std.mem.Allocator) void {
        std.crypto.secureZero(u8, self.api_key);
        alloc.free(self.api_key);
        alloc.free(self.provider);
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
        log.warn("tenant_provider.bad_mode tenant_id={s} mode={s}", .{ tenant_id, mode_label });
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
    if (std.mem.eql(u8, label, "byok")) return .byok;
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
/// Multi-workspace tenants point BYOK credentials at the first signup-time
/// workspace; v3 may add an explicit `vault_workspace_id` column to
/// tenant_providers so users can pin a different workspace.
pub fn resolvePrimaryWorkspace(
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

pub fn probeByokCredential(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    tenant_id: []const u8,
    credential_ref: []const u8,
) (ResolveError || anyerror)!ProbedCredential {
    const ws_id = try resolvePrimaryWorkspace(alloc, conn, tenant_id);
    defer alloc.free(ws_id);

    var parsed = vault.loadJson(alloc, conn, ws_id, credential_ref) catch |err| switch (err) {
        error.NotFound => return ResolveError.CredentialMissing,
        vault.Error.MalformedPlaintext => return ResolveError.CredentialDataMalformed,
        else => return err,
    };
    defer parsed.deinit();

    const provider_v = parsed.value.object.get("provider") orelse return ResolveError.CredentialDataMalformed;
    const api_key_v = parsed.value.object.get("api_key") orelse return ResolveError.CredentialDataMalformed;
    if (provider_v != .string or api_key_v != .string) return ResolveError.CredentialDataMalformed;
    if (provider_v.string.len == 0 or api_key_v.string.len == 0) return ResolveError.CredentialDataMalformed;

    const provider = try alloc.dupe(u8, provider_v.string);
    errdefer alloc.free(provider);
    const api_key = try alloc.dupe(u8, api_key_v.string);
    return .{ .provider = provider, .api_key = api_key };
}

pub fn loadVaultApiKey(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    key_name: []const u8,
) ![]u8 {
    var parsed = vault.loadJson(alloc, conn, workspace_id, key_name) catch |err| switch (err) {
        error.NotFound => return ResolveError.PlatformKeyMissing,
        vault.Error.MalformedPlaintext => return ResolveError.PlatformKeyMissing,
        else => return err,
    };
    defer parsed.deinit();

    const api_key_v = parsed.value.object.get("api_key") orelse return ResolveError.PlatformKeyMissing;
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

pub fn resolveByok(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    tenant_id: []const u8,
    row: ProviderRow,
) (ResolveError || anyerror)!ResolvedProvider {
    const credential_ref = row.credential_ref orelse return ResolveError.CredentialDataMalformed;
    var cred = try probeByokCredential(alloc, conn, tenant_id, credential_ref);
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

    return .{
        .mode = .byok,
        .provider = provider,
        .api_key = api_key,
        .model = model,
        .context_cap_tokens = row.context_cap_tokens,
    };
}
