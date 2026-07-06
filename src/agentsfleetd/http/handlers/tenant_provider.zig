//! /v1/tenants/me/provider — tenant-scoped LLM provider configuration.
//!
//! GET    returns the persisted config (no api_key, ever).
//! PUT    body {mode, secret_ref?, model?} validates eagerly and UPSERTs
//!        the row. Validation order matches the spec PUT contract:
//!          1. body shape malformed                          → 400 UZ-REQ-001
//!          2. mode=self_managed + secret_ref absent     → 400 UZ-PROVIDER-001
//!          3. mode=self_managed + credential row absent     → 400 UZ-PROVIDER-002
//!          4. mode=self_managed + JSON shape invalid        → 400 UZ-PROVIDER-003
//!          5. effective model not in caps catalogue         → 400 UZ-PROVIDER-004
//!          6. UPSERT, return 200 with the resolved config
//! DELETE is equivalent to PUT mode=platform — writes the explicit
//!        platform-default row so the dashboard can distinguish "never
//!        configured" from "explicitly reset".

const std = @import("std");
const logging = @import("log");
const httpz = @import("httpz");
const pg = @import("pg");
const PgQuery = @import("../../db/pg_query.zig").PgQuery;

const common = @import("common.zig");
const hx_mod = @import("hx.zig");
const ec = @import("../../errors/error_registry.zig");
const tenant_provider = @import("../../state/tenant_provider.zig");
const model_rate_cache = @import("../../state/model_rate_cache.zig");

const Hx = hx_mod.Hx;

const log = logging.scoped(.http_tenant_provider);

const S_PLATFORM = "platform";
const S_TENANT_CONTEXT_REQUIRED = "Tenant context required";

/// Context-cap persisted for a custom (openai-compatible) self-managed endpoint.
/// A custom endpoint bills provider-direct — self_managed posture charges a
/// run-fee only and never reads the per-token rate cache — so its user-hosted
/// model is absent from core.model_library by design and there is no platform rate
/// to catalogue. The activation gate stores this "unknown/auto" sentinel instead
/// of a catalogue lookup; execution_policy.autoToolWindow + the per-fleet
/// frontmatter overlay resolve the effective context window at run time.
const CUSTOM_ENDPOINT_CAP_UNKNOWN: u32 = 0;

const PutInput = struct {
    mode: []const u8,
    secret_ref: ?[]const u8 = null,
    model: ?[]const u8 = null,
};

// ── GET ─────────────────────────────────────────────────────────────────────

pub fn innerGetTenantProvider(hx: Hx, req: *httpz.Request) void {
    _ = req;
    const tenant_id = hx.principal.tenant_id orelse {
        hx.fail(ec.ERR_FORBIDDEN, S_TENANT_CONTEXT_REQUIRED);
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
        hx.fail(ec.ERR_FORBIDDEN, S_TENANT_CONTEXT_REQUIRED);
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

    if (std.mem.eql(u8, input.mode, S_PLATFORM)) {
        applyPlatform(hx, conn, tenant_id);
        return;
    }
    if (std.mem.eql(u8, input.mode, "self_managed")) {
        applySelfManaged(hx, conn, tenant_id, input);
        return;
    }
    hx.fail(ec.ERR_INVALID_REQUEST, "mode must be 'platform' or 'self_managed'");
}

// ── DELETE ──────────────────────────────────────────────────────────────────

pub fn innerDeleteTenantProvider(hx: Hx, req: *httpz.Request) void {
    _ = req;
    const tenant_id = hx.principal.tenant_id orelse {
        hx.fail(ec.ERR_FORBIDDEN, S_TENANT_CONTEXT_REQUIRED);
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

fn applyPlatform(hx: Hx, conn: *pg.Conn, tenant_id: []const u8) void {
    tenant_provider.upsertPlatform(hx.alloc, conn, tenant_id) catch |err| switch (err) {
        tenant_provider.ResolveError.PlatformKeyMissing => {
            log.err("platform_missing", .{ .error_code = ec.ERR_PROVIDER_PLATFORM_KEY_MISSING, .tenant_id = tenant_id });
            // `detail` is wire-visible (writeProblem's JSON body) — unlike the
            // registry entry's `hint`, it must not leak internal schema/table
            // names to a tenant-scoped caller (this handler only requires
            // SECRET_WRITE, not a platform-operator scope).
            common.errorResponse(hx.res, ec.ERR_PROVIDER_PLATFORM_KEY_MISSING, "Platform LLM key not configured", hx.req_id);
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

fn applySelfManaged(hx: Hx, conn: *pg.Conn, tenant_id: []const u8, input: PutInput) void {
    const secret_ref = input.secret_ref orelse {
        hx.fail(ec.ERR_PROVIDER_SECRET_REF_REQUIRED, "secret_ref required when mode=self_managed");
        return;
    };

    var probed = tenant_provider.probeSelfManaged(hx.alloc, conn, tenant_id, secret_ref) catch |err| switch (err) {
        tenant_provider.ResolveError.SecretMissing => {
            hx.fail(ec.ERR_PROVIDER_SECRET_NOT_FOUND, "credential row not found in vault");
            return;
        },
        tenant_provider.ResolveError.SecretDataMalformed => {
            hx.fail(ec.ERR_PROVIDER_SECRET_DATA_MALFORMED, "credential JSON missing required field (provider, api_key, or model)");
            return;
        },
        tenant_provider.ResolveError.SecretEndpointInvalid => {
            hx.fail(ec.ERR_PROVIDER_BASE_URL_INVALID, "custom endpoint base_url is missing, not https, SSRF-unsafe, or set on a non-openai-compatible provider");
            return;
        },
        tenant_provider.ResolveError.TenantHasNoWorkspace => {
            log.err("no_workspace", .{ .error_code = ec.ERR_TENANT_NO_PRIMARY_WORKSPACE, .tenant_id = tenant_id });
            common.errorResponse(hx.res, ec.ERR_TENANT_NO_PRIMARY_WORKSPACE, "Tenant has no primary workspace configured", hx.req_id);
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
    const context_cap_tokens = resolveSelfManagedCap(probed.provider, effective_model) orelse {
        hx.fail(ec.ERR_PROVIDER_MODEL_NOT_IN_CATALOGUE, "model not in cached caps catalogue");
        return;
    };

    tenant_provider.upsertSelfManaged(hx.alloc, conn, tenant_id, secret_ref, effective_model, context_cap_tokens) catch |err| {
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

/// Resolve the context-window cap to persist for a self-managed activation.
/// A custom (openai-compatible) endpoint is provider-direct billing: its
/// user-hosted model is absent from the platform rate catalogue by design, so it
/// bypasses the gate and takes the unknown/auto sentinel. A named provider must
/// resolve a catalogued rate row (whose cap we store) — `null` means the model is
/// not in the catalogue, and the caller fails it (UZ-PROVIDER-004). The rate row
/// is keyed by (provider, model): the credential's provider is the authority for
/// which provider hosts the model.
fn resolveSelfManagedCap(provider: []const u8, model: []const u8) ?u32 {
    if (std.mem.eql(u8, provider, tenant_provider.OPENAI_COMPATIBLE_PROVIDER)) {
        return CUSTOM_ENDPOINT_CAP_UNKNOWN;
    }
    const entry = model_rate_cache.lookup_model_rate(provider, model) orelse return null;
    return entry.context_cap_tokens;
}

const ProviderView = struct {
    mode: []const u8,
    provider: []const u8,
    model: []const u8,
    context_cap_tokens: u32,
    secret_ref: ?[]const u8,
};

fn readProviderView(alloc: std.mem.Allocator, conn: *pg.Conn, tenant_id: []const u8) !ProviderView {
    var q = PgQuery.from(try conn.query(
        \\SELECT mode, provider, model, context_cap_tokens, secret_ref
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
            .secret_ref = cred_ref,
        };
    }
    // No explicit row → the tenant runs on the live platform default. Source it
    // from the active platform key row (provider/model/cap), not a constant, so
    // this view tracks whatever the admin set in /admin/models.
    const mode = try alloc.dupe(u8, S_PLATFORM);
    errdefer alloc.free(mode);

    if (try tenant_provider.platformDefaultView(alloc, conn)) |dv| {
        var view = dv;
        errdefer view.deinit(alloc);
        return .{
            .mode = mode,
            .provider = view.provider,
            .model = view.model,
            .context_cap_tokens = view.context_cap_tokens,
            .secret_ref = null,
        };
    }

    // No platform default configured yet — report platform mode with empty
    // provider/model so the dashboard shows "not configured" rather than a
    // stale hardcoded model.
    const provider = try alloc.dupe(u8, "");
    errdefer alloc.free(provider);
    const model = try alloc.dupe(u8, "");
    return .{
        .mode = mode,
        .provider = provider,
        .model = model,
        .context_cap_tokens = 0,
        .secret_ref = null,
    };
}

fn freeView(alloc: std.mem.Allocator, view: ProviderView) void {
    alloc.free(view.mode);
    alloc.free(view.provider);
    alloc.free(view.model);
    if (view.secret_ref) |c| alloc.free(c);
}
