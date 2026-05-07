//! /v1/tenants/me/provider — tenant-scoped LLM provider configuration.
//!
//! GET    returns the persisted config (no api_key, ever).
//! PUT    body {mode, credential_ref?, model?} validates eagerly and UPSERTs
//!        the row. Validation order matches the spec PUT contract:
//!          1. body shape malformed                  → 400 UZ-REQ-001
//!          2. mode=byok + credential_ref absent     → 400 UZ-PROVIDER-001
//!          3. mode=byok + credential row absent     → 400 UZ-PROVIDER-002
//!          4. mode=byok + JSON shape invalid        → 400 UZ-PROVIDER-003
//!          5. effective model not in caps catalogue → 400 UZ-PROVIDER-004
//!          6. UPSERT, return 200 with the resolved config
//! DELETE is equivalent to PUT mode=platform — writes the explicit
//!        platform-default row so the dashboard can distinguish "never
//!        configured" from "explicitly reset".

const std = @import("std");
const logging = @import("log");
const httpz = @import("httpz");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;

const common = @import("common.zig");
const hx_mod = @import("hx.zig");
const ec = @import("../../errors/error_registry.zig");
const tenant_provider = @import("../../state/tenant_provider.zig");
const model_rate_cache = @import("../../state/model_rate_cache.zig");

const Hx = hx_mod.Hx;

const log = logging.scoped(.http_tenant_provider);

const PutInput = struct {
    mode: []const u8,
    credential_ref: ?[]const u8 = null,
    model: ?[]const u8 = null,
};

// ── GET ─────────────────────────────────────────────────────────────────────

pub fn innerGetTenantProvider(hx: Hx, req: *httpz.Request) void {
    _ = req;
    const tenant_id = hx.principal.tenant_id orelse {
        hx.fail(ec.ERR_FORBIDDEN, "Tenant context required");
        return;
    };

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const view = readProviderView(hx.alloc, conn, tenant_id) catch |err| {
        log.err("get_failed", .{ .error_code = ec.ERR_INTERNAL_DB_UNAVAILABLE, .tenant_id = tenant_id, .err = @errorName(err) });
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer freeView(hx.alloc, view);

    hx.ok(.ok, view);
}

// ── PUT ─────────────────────────────────────────────────────────────────────

pub fn innerPutTenantProvider(hx: Hx, req: *httpz.Request) void {
    const tenant_id = hx.principal.tenant_id orelse {
        hx.fail(ec.ERR_FORBIDDEN, "Tenant context required");
        return;
    };

    const body = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, "Request body required");
        return;
    };
    const parsed = std.json.parseFromSlice(PutInput, hx.alloc, body, .{
        .ignore_unknown_fields = true,
    }) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "Malformed JSON");
        return;
    };
    defer parsed.deinit();
    const input = parsed.value;

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (std.mem.eql(u8, input.mode, "platform")) {
        applyPlatform(hx, conn, tenant_id);
        return;
    }
    if (std.mem.eql(u8, input.mode, "byok")) {
        applyByok(hx, conn, tenant_id, input);
        return;
    }
    hx.fail(ec.ERR_INVALID_REQUEST, "mode must be 'platform' or 'byok'");
}

// ── DELETE ──────────────────────────────────────────────────────────────────

pub fn innerDeleteTenantProvider(hx: Hx, req: *httpz.Request) void {
    _ = req;
    const tenant_id = hx.principal.tenant_id orelse {
        hx.fail(ec.ERR_FORBIDDEN, "Tenant context required");
        return;
    };

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    applyPlatform(hx, conn, tenant_id);
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn applyPlatform(hx: Hx, conn: *@import("pg").Conn, tenant_id: []const u8) void {
    tenant_provider.upsertPlatform(hx.alloc, conn, tenant_id) catch |err| switch (err) {
        tenant_provider.ResolveError.PlatformKeyMissing => {
            log.err("platform_missing", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .tenant_id = tenant_id });
            common.internalOperationError(hx.res, "Platform LLM key not configured — operator action required", hx.req_id);
            return;
        },
        else => {
            log.err("platform_failed", .{ .error_code = ec.ERR_INTERNAL_DB_UNAVAILABLE, .tenant_id = tenant_id, .err = @errorName(err) });
            common.internalDbUnavailable(hx.res, hx.req_id);
            return;
        },
    };

    const view = readProviderView(hx.alloc, conn, tenant_id) catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer freeView(hx.alloc, view);
    hx.ok(.ok, view);
}

fn applyByok(hx: Hx, conn: *@import("pg").Conn, tenant_id: []const u8, input: PutInput) void {
    const credential_ref = input.credential_ref orelse {
        hx.fail(ec.ERR_PROVIDER_CREDENTIAL_REF_REQUIRED, "credential_ref required when mode=byok");
        return;
    };

    var probed = tenant_provider.probeByok(hx.alloc, conn, tenant_id, credential_ref) catch |err| switch (err) {
        tenant_provider.ResolveError.CredentialMissing => {
            hx.fail(ec.ERR_PROVIDER_CREDENTIAL_NOT_FOUND, "credential row not found in vault");
            return;
        },
        tenant_provider.ResolveError.CredentialDataMalformed => {
            hx.fail(ec.ERR_PROVIDER_CREDENTIAL_DATA_MALFORMED, "credential JSON missing required field (provider, api_key, or model)");
            return;
        },
        tenant_provider.ResolveError.TenantHasNoWorkspace => {
            log.err("no_workspace", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .tenant_id = tenant_id });
            common.internalOperationError(hx.res, "Tenant has no primary workspace — bootstrap invariant violated", hx.req_id);
            return;
        },
        else => {
            log.err("probe_failed", .{ .error_code = ec.ERR_INTERNAL_DB_UNAVAILABLE, .tenant_id = tenant_id, .err = @errorName(err) });
            common.internalDbUnavailable(hx.res, hx.req_id);
            return;
        },
    };
    defer probed.deinit(hx.alloc);

    // Effective model: caller's --model override OR the credential's stored model.
    const effective_model: []const u8 = input.model orelse probed.model;
    const cache_entry = model_rate_cache.lookup_model_rate(effective_model) orelse {
        hx.fail(ec.ERR_PROVIDER_MODEL_NOT_IN_CATALOGUE, "model not in cached caps catalogue");
        return;
    };

    tenant_provider.upsertByok(hx.alloc, conn, tenant_id, credential_ref, effective_model, cache_entry.context_cap_tokens) catch |err| {
        log.err("upsert_failed", .{ .error_code = ec.ERR_INTERNAL_DB_UNAVAILABLE, .tenant_id = tenant_id, .err = @errorName(err) });
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };

    const view = readProviderView(hx.alloc, conn, tenant_id) catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer freeView(hx.alloc, view);
    hx.ok(.ok, view);
}

const ProviderView = struct {
    mode: []const u8,
    provider: []const u8,
    model: []const u8,
    context_cap_tokens: u32,
    credential_ref: ?[]const u8,
};

fn readProviderView(alloc: std.mem.Allocator, conn: *@import("pg").Conn, tenant_id: []const u8) !ProviderView {
    var q = PgQuery.from(try conn.query(
        \\SELECT mode, provider, model, context_cap_tokens, credential_ref
        \\FROM core.tenant_providers
        \\WHERE tenant_id = $1::uuid
    , .{tenant_id}));
    defer q.deinit();
    if (try q.next()) |row| {
        const mode = try alloc.dupe(u8, try row.get([]const u8, 0));
        errdefer alloc.free(mode);
        const provider = try alloc.dupe(u8, try row.get([]const u8, 1));
        errdefer alloc.free(provider);
        const model = try alloc.dupe(u8, try row.get([]const u8, 2));
        errdefer alloc.free(model);
        const cap_i32 = try row.get(i32, 3);
        const cred_opt = try row.get(?[]const u8, 4);
        const cred_ref: ?[]const u8 = if (cred_opt) |c| try alloc.dupe(u8, c) else null;
        return .{
            .mode = mode,
            .provider = provider,
            .model = model,
            .context_cap_tokens = @intCast(@max(cap_i32, 0)),
            .credential_ref = cred_ref,
        };
    }
    // Synth platform default for tenants with no row.
    const mode = try alloc.dupe(u8, "platform");
    errdefer alloc.free(mode);
    const provider = try alloc.dupe(u8, "fireworks");
    errdefer alloc.free(provider);
    const model = try alloc.dupe(u8, tenant_provider.PLATFORM_DEFAULT_MODEL);
    return .{
        .mode = mode,
        .provider = provider,
        .model = model,
        .context_cap_tokens = tenant_provider.PLATFORM_DEFAULT_CAP_TOKENS,
        .credential_ref = null,
    };
}

fn freeView(alloc: std.mem.Allocator, view: ProviderView) void {
    alloc.free(view.mode);
    alloc.free(view.provider);
    alloc.free(view.model);
    if (view.credential_ref) |c| alloc.free(c);
}
