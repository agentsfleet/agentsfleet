//! /v1/tenants/me/models — tenant-scoped many-model registry (M121 §2).
//!
//! GET    lists every entry joined to its secret's non-secret metadata, with
//!        `active` computed against the tenant's current selection, plus
//!        `platform_default_available` and — when a default is active — its
//!        identity as `platform_default` {provider, model, context_cap_tokens}.
//!        Pure read — activation itself (tenant_provider.zig) upserts the
//!        matching entry, so the selection always has one. See
//!        tenant_model_entries_view.zig.
//! POST   {model_id, secret_ref} — 404 UZ-MODELS-002 (unknown secret),
//!        409 UZ-MODELS-003 (duplicate).
//! PATCH  {model_id} — model change only; secret_ref is immutable here.
//!        404 UZ-MODELS-004 when the id doesn't resolve for this tenant.
//! DELETE refuses the active entry (409 UZ-MODELS-001); otherwise idempotent
//!        204, matching fleets/secrets.zig's innerDeleteSecret convention.
//!
//! Activation is NOT new surface — PUT /v1/tenants/me/provider (unchanged)
//! remains the only path that flips the tenant's active selection.

const std = @import("std");
const logging = @import("log");
const httpz = @import("httpz");
const pg = @import("pg");

const common = @import("common.zig");
const hx_mod = @import("hx.zig");
const ec = @import("../../errors/error_registry.zig");
const id_format = @import("../../types/id_format.zig");
const entries_state = @import("../../state/tenant_model_entries.zig");
const tenant_provider = @import("../../state/tenant_provider.zig");
const view = @import("tenant_model_entries_view.zig");

const Hx = hx_mod.Hx;
const log = logging.scoped(.http_tenant_model_entries);

const S_TENANT_CONTEXT_REQUIRED = "Tenant context required";
const S_MODEL_ID_REQUIRED = "model_id is required";
const S_SECRET_REF_REQUIRED = "secret_ref is required";
const S_ID_MUST_BE_UUIDV7 = "id must be a valid UUIDv7";
const S_BODY_REQUIRED = "Request body required";
const S_MALFORMED_JSON = "Malformed JSON";
const S_DUPLICATE_DETAIL = "An entry with this model and secret already exists";

// ── GET ─────────────────────────────────────────────────────────────────────

pub fn innerListModelEntries(hx: Hx, req: *httpz.Request) void {
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

    var result = view.buildList(hx.alloc, conn, tenant_id) catch |err| {
        log.err("list_failed", .{ .error_code = ec.ERR_INTERNAL_DB_UNAVAILABLE, .tenant_id = tenant_id, .err = @errorName(err) });
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer result.deinit(hx.alloc);

    hx.res.status = @intFromEnum(std.http.Status.ok);
    hx.res.json(
        .{
            .models = result.rows,
            .platform_default_available = result.platform_default_available,
            .platform_default = result.platform_default,
        },
        .{ .emit_null_optional_fields = false },
    ) catch {
        common.internalOperationError(hx.res, "Failed to build the models list", hx.req_id);
    };
}

// ── POST ────────────────────────────────────────────────────────────────────

const CreateBody = struct {
    model_id: []const u8,
    secret_ref: []const u8,
};

pub fn innerCreateModelEntry(hx: Hx, req: *httpz.Request) void {
    const tenant_id = hx.principal.tenant_id orelse {
        hx.fail(ec.ERR_FORBIDDEN, S_TENANT_CONTEXT_REQUIRED);
        return;
    };

    const body = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, S_BODY_REQUIRED);
        return;
    };
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return;
    const parsed = std.json.parseFromSlice(CreateBody, hx.alloc, body, .{ .ignore_unknown_fields = true }) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, S_MALFORMED_JSON);
        return;
    };
    defer parsed.deinit();
    const input = parsed.value;
    if (!validateCreateBody(hx, input)) return;

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const exists = entries_state.secretExistsForTenant(conn, tenant_id, input.secret_ref) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    if (!exists) {
        hx.fail(ec.ERR_MODELS_SECRET_NOT_FOUND, "secret_ref does not name a vault secret in this tenant's workspace");
        return;
    }

    performCreate(hx, conn, tenant_id, input);
}

fn validateCreateBody(hx: Hx, input: CreateBody) bool {
    if (input.model_id.len == 0) {
        hx.fail(ec.ERR_INVALID_REQUEST, S_MODEL_ID_REQUIRED);
        return false;
    }
    if (input.secret_ref.len == 0) {
        hx.fail(ec.ERR_INVALID_REQUEST, S_SECRET_REF_REQUIRED);
        return false;
    }
    return true;
}

fn performCreate(hx: Hx, conn: *pg.Conn, tenant_id: []const u8, input: CreateBody) void {
    const new_id = id_format.generateTenantModelEntryId(hx.alloc) catch {
        common.internalOperationError(hx.res, "Failed to mint an entry id", hx.req_id);
        return;
    };
    defer hx.alloc.free(new_id);

    var created = entries_state.create(hx.alloc, conn, .{
        .id = new_id,
        .tenant_id = tenant_id,
        .model_id = input.model_id,
        .secret_ref = input.secret_ref,
    }) catch |err| switch (err) {
        entries_state.StateError.DuplicateEntry => {
            hx.fail(ec.ERR_MODELS_DUPLICATE_ENTRY, S_DUPLICATE_DETAIL);
            return;
        },
        else => {
            log.err("create_failed", .{ .error_code = ec.ERR_INTERNAL_DB_UNAVAILABLE, .tenant_id = tenant_id, .err = @errorName(err) });
            common.internalDbUnavailable(hx.res, hx.req_id);
            return;
        },
    };
    defer created.deinit(hx.alloc);

    hx.ok(.created, .{
        .id = created.id,
        .model_id = created.model_id,
        .secret_ref = created.secret_ref,
        .created_at = created.created_at,
    });
}

// ── PATCH ───────────────────────────────────────────────────────────────────

const UpdateBody = struct {
    model_id: []const u8,
};

pub fn innerUpdateModelEntry(hx: Hx, req: *httpz.Request, entry_id: []const u8) void {
    const tenant_id = hx.principal.tenant_id orelse {
        hx.fail(ec.ERR_FORBIDDEN, S_TENANT_CONTEXT_REQUIRED);
        return;
    };
    if (!id_format.isUuidV7(entry_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, S_ID_MUST_BE_UUIDV7);
        return;
    }

    const body = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, S_BODY_REQUIRED);
        return;
    };
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return;
    const parsed = std.json.parseFromSlice(UpdateBody, hx.alloc, body, .{ .ignore_unknown_fields = true }) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, S_MALFORMED_JSON);
        return;
    };
    defer parsed.deinit();
    if (parsed.value.model_id.len == 0) {
        hx.fail(ec.ERR_INVALID_REQUEST, S_MODEL_ID_REQUIRED);
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    var updated = entries_state.updateModel(hx.alloc, conn, tenant_id, entry_id, parsed.value.model_id) catch |err| switch (err) {
        entries_state.StateError.NotFound => {
            hx.fail(ec.ERR_MODELS_ENTRY_NOT_FOUND, "Model entry not found");
            return;
        },
        entries_state.StateError.DuplicateEntry => {
            hx.fail(ec.ERR_MODELS_DUPLICATE_ENTRY, S_DUPLICATE_DETAIL);
            return;
        },
        else => {
            log.err("update_failed", .{ .error_code = ec.ERR_INTERNAL_DB_UNAVAILABLE, .tenant_id = tenant_id, .err = @errorName(err) });
            common.internalDbUnavailable(hx.res, hx.req_id);
            return;
        },
    };
    defer updated.deinit(hx.alloc);

    hx.ok(.ok, .{
        .id = updated.id,
        .model_id = updated.model_id,
        .secret_ref = updated.secret_ref,
        .created_at = updated.created_at,
    });
}

// ── DELETE ──────────────────────────────────────────────────────────────────

pub fn innerDeleteModelEntry(hx: Hx, req: *httpz.Request, entry_id: []const u8) void {
    _ = req;
    const tenant_id = hx.principal.tenant_id orelse {
        hx.fail(ec.ERR_FORBIDDEN, S_TENANT_CONTEXT_REQUIRED);
        return;
    };
    if (!id_format.isUuidV7(entry_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, S_ID_MUST_BE_UUIDV7);
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    const is_active = isActiveEntry(hx.alloc, conn, tenant_id, entry_id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    if (is_active) {
        hx.fail(ec.ERR_MODELS_DELETE_ACTIVE, "This entry is the tenant's active selection; switch to another entry first");
        return;
    }

    // Idempotent — a missing id (already deleted, or never existed) still 204s,
    // matching fleets/secrets.zig's innerDeleteSecret.
    _ = entries_state.delete(conn, tenant_id, entry_id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    hx.noContent();
}

/// Whether `entry_id` is the entry backing the tenant's current self-managed
/// selection. No `active` column exists on the row — the comparison is by
/// (secret_ref, model_id) against `core.tenant_model_selection`, same as the
/// list view's `active` flag.
fn isActiveEntry(alloc: std.mem.Allocator, conn: *pg.Conn, tenant_id: []const u8, entry_id: []const u8) !bool {
    var selection = (try tenant_provider.activeSelfManagedRef(alloc, conn, tenant_id)) orelse return false;
    defer selection.deinit(alloc);

    const entries = try entries_state.list(alloc, conn, tenant_id);
    defer entries_state.deinitEntryList(entries, alloc);
    for (entries) |e| {
        if (!std.mem.eql(u8, e.id, entry_id)) continue;
        return std.mem.eql(u8, e.secret_ref, selection.secret_ref) and std.mem.eql(u8, e.model_id, selection.model);
    }
    return false;
}
