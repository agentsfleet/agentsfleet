//! DELETE /v1/tenants/me/models/{id} — removing a registry entry.
//!
//! Split from tenant_model_entries.zig under RULE FLL when the removal path
//! took on the shared reference-lock protocol. The seam is not arbitrary: this
//! file owns REMOVING an entry, which is the one verb that has to reason about
//! the active selection and about a credential it does not own, while the other
//! file owns creating and renaming one.
//!
//! The active-check and the DELETE are one transaction under the shared lock
//! order (credential → entries → selection). They used to be two unsynchronized
//! statements, which let an activation commit between them and leave the
//! selection naming a row this handler had just deleted.

const std = @import("std");
const httpz = @import("httpz");
const pg = @import("pg");

const common = @import("common.zig");
const hx_mod = @import("hx.zig");
const ec = @import("../../errors/error_registry.zig");
const id_format = @import("../../types/id_format.zig");
const entries_state = @import("../../state/tenant_model_entries.zig");
const secret_probe = @import("../../state/secret_probe.zig");
const secret_reference_txn = @import("../../state/secret_reference_txn.zig");
const tenant_provider = @import("../../state/tenant_provider.zig");

const Hx = hx_mod.Hx;

const S_TENANT_CONTEXT_REQUIRED = "Tenant context required";
const S_ID_MUST_BE_UUIDV7 = "id must be a valid UUIDv7";
const S_DELETE_ACTIVE = "This entry is the tenant's active selection; switch to another entry first";

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

    // Name the credential before locking: the shared order starts at the vault
    // row, so the row we are deleting has to tell us which one.
    //
    // Idempotent — a missing id (already deleted, or never existed) still 204s,
    // matching fleets/secrets.zig's innerDeleteSecret.
    var entry = (entries_state.getById(hx.alloc, conn, tenant_id, entry_id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    }) orelse {
        hx.noContent();
        return;
    };
    defer entry.deinit(hx.alloc);

    const ws_id = secret_probe.resolvePrimaryWorkspace(hx.alloc, conn, tenant_id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    defer hx.alloc.free(ws_id);

    // The active-check and the DELETE must be one act. They used to be two
    // unsynchronized statements, which let an activation commit in the gap: the
    // check saw an inactive entry, activation made it the selection, and the
    // delete then removed the row the selection names — leaving an active
    // selection with no registry entry, the exact M121 invariant `ensureEntry`
    // exists to hold.
    //
    // This takes the SHARED order (credential → entries → selection) rather
    // than locking the selection alone. Locking only the selection would invert
    // the order against activation, which takes the selection last, and an
    // inverted lock order is a deadlock rather than a visible bug.
    var txn = secret_reference_txn.begin(conn, ws_id, entry.secret_ref) catch |err| switch (err) {
        // No credential, so no reference race to lose: this entry is already an
        // orphan and removing it is exactly the right cleanup.
        secret_reference_txn.Error.SecretGone => {
            _ = entries_state.delete(conn, tenant_id, entry_id) catch {
                common.internalDbError(hx.res, hx.req_id);
                return;
            };
            hx.noContent();
            return;
        },
        else => {
            common.internalDbError(hx.res, hx.req_id);
            return;
        },
    };
    defer txn.abort();

    const is_active = isActiveEntry(hx.alloc, conn, tenant_id, entry_id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    if (is_active) {
        hx.fail(ec.ERR_MODELS_DELETE_ACTIVE, S_DELETE_ACTIVE);
        return;
    }

    _ = entries_state.delete(conn, tenant_id, entry_id) catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    txn.commit() catch {
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
