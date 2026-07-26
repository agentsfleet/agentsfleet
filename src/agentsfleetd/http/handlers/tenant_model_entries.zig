//! /v1/tenants/me/models — tenant-scoped many-model registry.
//!
//! GET    lists every entry joined to its secret's non-secret metadata, with
//!        `active` computed against the tenant's current selection, plus
//!        `platform_default_available` and — when a default is active — its
//!        identity as `platform_default` {provider, model, context_cap_tokens, rates}.
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
const secret_probe = @import("../../state/secret_probe.zig");
const secret_reference_txn = @import("../../state/secret_reference_txn.zig");
const model_identity = @import("../../types/model_identity.zig");

/// One rule, two call sites (POST and PATCH), so the bound cannot hold on one
/// verb and not the other — which is exactly how `model_id` ended up bounded on
/// the catalogue route and unbounded here.
fn modelIdRejected(hx: Hx, model_id: []const u8) bool {
    if (model_id.len == 0) {
        hx.fail(ec.ERR_INVALID_REQUEST, S_MODEL_ID_REQUIRED);
        return true;
    }
    if (model_id.len > model_identity.MODEL_ID_MAX) {
        hx.fail(ec.ERR_INVALID_REQUEST, S_MODEL_ID_TOO_LONG);
        return true;
    }
    return false;
}

const Hx = hx_mod.Hx;
const log = logging.scoped(.http_tenant_model_entries);

const S_TENANT_CONTEXT_REQUIRED = "Tenant context required";
const S_MODEL_ID_REQUIRED = "model_id is required";
const S_MODEL_ID_TOO_LONG = "model_id must be at most 256 chars";
const S_SECRET_REF_REQUIRED = "secret_ref is required";
const S_ID_MUST_BE_UUIDV7 = "id must be a valid UUIDv7";
const S_BODY_REQUIRED = "Request body required";
const S_MALFORMED_JSON = "Malformed JSON";
const S_DUPLICATE_DETAIL = "An entry with this model and secret already exists";
const S_SECRET_REF_UNKNOWN = "secret_ref does not name a vault secret in this tenant's workspace";

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

    // The workspace the credential actually lives in — the reference lock is
    // taken on (workspace_id, key_name), which is the vault's identity, not the
    // tenant's.
    const ws_id = secret_probe.resolvePrimaryWorkspace(hx.alloc, conn, tenant_id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    defer hx.alloc.free(ws_id);

    // Lock the credential BEFORE deciding it exists, so the decision and the
    // insert are one atomic act. The previous shape checked existence and then
    // inserted with nothing held between, which let a concurrent
    // `DELETE /workspaces/{ws}/secrets/{name}` remove the credential in the gap
    // and leave this entry pointing at nothing (state/secret_reference_txn.zig).
    var txn = secret_reference_txn.begin(conn, ws_id, input.secret_ref, tenant_id) catch |err| switch (err) {
        // Absent covers both "never existed" and "deleted a moment ago". Both
        // mean the same thing to this caller and neither is retryable by simply
        // re-sending, so the existing 404 stays the answer rather than the
        // 409 that a lost race would suggest.
        secret_reference_txn.Error.SecretGone => {
            hx.fail(ec.ERR_MODELS_SECRET_NOT_FOUND, S_SECRET_REF_UNKNOWN);
            return;
        },
        else => {
            common.internalDbError(hx.res, hx.req_id);
            return;
        },
    };
    errdefer txn.abort();

    performCreate(hx, conn, tenant_id, input, &txn);
}

fn validateCreateBody(hx: Hx, input: CreateBody) bool {
    if (modelIdRejected(hx, input.model_id)) return false;
    if (input.secret_ref.len == 0) {
        hx.fail(ec.ERR_INVALID_REQUEST, S_SECRET_REF_REQUIRED);
        return false;
    }
    return true;
}

/// Insert the entry inside `txn`, which already holds the credential's row
/// lock. Every exit path either commits or aborts before returning.
fn performCreate(hx: Hx, conn: *pg.Conn, tenant_id: []const u8, input: CreateBody, txn: *secret_reference_txn.Txn) void {
    errdefer txn.abort();

    const new_id = id_format.generateTenantModelEntryId(hx.alloc) catch {
        txn.abort();
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
            txn.abort();
            hx.fail(ec.ERR_MODELS_DUPLICATE_ENTRY, S_DUPLICATE_DETAIL);
            return;
        },
        else => {
            txn.abort();
            log.err("create_failed", .{ .error_code = ec.ERR_INTERNAL_DB_UNAVAILABLE, .tenant_id = tenant_id, .err = @errorName(err) });
            common.internalDbUnavailable(hx.res, hx.req_id);
            return;
        },
    };
    defer created.deinit(hx.alloc);

    // Commit BEFORE responding. A 201 whose transaction then fails to commit is
    // the worst outcome available: the client records an id that does not exist.
    txn.commit() catch |err| {
        log.err("create_commit_failed", .{ .error_code = ec.ERR_INTERNAL_DB_UNAVAILABLE, .tenant_id = tenant_id, .err = @errorName(err) });
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };

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
    if (modelIdRejected(hx, parsed.value.model_id)) return;

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
