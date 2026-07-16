//! PATCH /v1/workspaces/{ws}/fleets/{id} — partial update of a fleet.
//!
//! Body fields (all optional, presence-based; empty body → 200 no-op):
//!   - `config_json`      — replace config_json blob directly.
//!   - `status`           — "active" | "stopped" | "killed"; drives the FSM.
//!   - `trigger_markdown` — reparses; rewrites trigger_markdown + config_json + name.
//!   - `source_markdown`  — reparses; validates name match; rewrites source_markdown.
//!
//! `config_json` and `trigger_markdown` are mutually exclusive (both drive
//! `core.fleets.config_json`). Body parsing + validation live in
//! `patch_body.zig`; the transaction and the status FSM live in `patch_txn.zig`.
//!
//! Optimistic concurrency: a caller may send `If-Match: <etag>` carrying the
//! `ETag` the single-fleet read returned (a hash of the editable markdown —
//! see `etag.zig`). A stale tag is a 412 whose body carries the current one,
//! so two operators editing the same source can never silently overwrite each
//! other. Every 200 response carries the post-update `ETag`.
//!
//! Status FSM (paused is gate-only, never set via API):
//!     active|paused|stopped → stopped  (resume from auto-pause/operator-stop)
//!     active|paused|stopped → active
//!     active|paused|stopped → killed   (terminal — 404 on further PATCH)
//!     same → same          → 409 (no-op transition rejected by the SQL guard)
//!
//! `updated_at` (BIGINT ms epoch) doubles as config_revision — monotonic.

const httpz = @import("httpz");
const logging = @import("log");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const etag_mod = @import("../../etag.zig");
const patch_body = @import("patch_body.zig");
const patch_txn = @import("patch_txn.zig");
const cron_sync = @import("cron_sync.zig");
const workspace_guards = @import("../../workspace_guards.zig");

const log = logging.scoped(.fleet_api);

const Hx = hx_mod.Hx;

/// Partial update of a fleet. Validates inputs, persists in one FSM-gated SQL
/// UPDATE, and answers with the post-update revision + ETag. Returns 404 if the
/// fleet is missing or already-killed (a killed fleet is a tombstone — no
/// further state changes apply), 409 if the requested status transition is not
/// allowed from the current state (e.g. resume on an active fleet), and 412 if
/// the caller's `If-Match` names a source version the row has moved past.
pub fn innerPatchFleet(hx: Hx, req: *httpz.Request, workspace_id: []const u8, fleet_id: []const u8) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    if (!id_format.isSupportedWorkspaceId(fleet_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "fleet_id must be a valid UUIDv7");
        return;
    }

    const body = patch_body.parsePatchBody(hx, req) orelse return;
    const if_match = etag_mod.ifMatch(req);
    if (body.config_json == null and body.status == null and
        body.trigger_markdown == null and body.source_markdown == null)
    {
        if (if_match != null) {
            hx.fail(ec.ERR_INVALID_REQUEST, "A conditional fleet update requires at least one field");
            return;
        }
        hx.ok(.ok, .{ .fleet_id = fleet_id, .config_revision = @as(?i64, null) });
        return;
    }
    if (!patch_body.validateBody(hx, body)) return;

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    // Status transitions (stop/resume/kill) take the ownership gate that also
    // writes the RLS tenant context (`enforce`), so a `workspace:any` cross-tenant
    // status change is audited. Pure config_json updates (no status field) take
    // the bare ownership check. Capability for both is gated upstream by the
    // route's `fleet:write` scope (requireScope), independent of this axis.
    if (body.status != null) {
        const access = workspace_guards.enforce(hx.res, hx.req_id, conn, hx.principal, workspace_id) orelse return;
        defer access.deinit(hx.alloc);
    } else {
        if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
            hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
            return;
        }
    }

    const outcome = patch_txn.patchFleetInTxn(hx.alloc, conn, workspace_id, fleet_id, body, if_match) catch |err| {
        log.err("patch_db_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err), .fleet_id = fleet_id, .req_id = hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    const updated = resolveOutcome(hx, fleet_id, outcome) orelse return;

    if (body.status != null or body.trigger_markdown != null or body.config_json != null) {
        const cron_result = cron_sync.syncStoredFleet(hx, workspace_id, fleet_id);
        if (cron_result != .ok and cron_result != .skipped) {
            _ = cron_sync.writeFailure(hx, cron_result);
            return;
        }
    }

    // No control-stream signal: `agentsfleetd` resolves a fleet's status + config
    // fresh from Postgres on every lease, so the PATCH'd row (already committed
    // above) takes effect on the next lease with nothing to notify.
    log.debug("patched", .{ .id = fleet_id, .workspace = workspace_id, .revision = updated.revision, .status_set = body.status });
    // The write is committed; only the response tag failed to attach. Answering
    // 500 without the ETag is the safe end: the editor refetches rather than
    // saving its next edit against a tag it never received.
    etag_mod.attach(hx.res, updated.etag) catch {
        common.internalOperationError(hx.res, "Failed to confirm this fleet's saved source", hx.req_id);
        return;
    };
    if (body.status) |s| {
        hx.ok(.ok, .{ .fleet_id = fleet_id, .status = s, .config_revision = updated.revision, .etag = updated.etag });
    } else {
        hx.ok(.ok, .{ .fleet_id = fleet_id, .config_revision = updated.revision, .etag = updated.etag });
    }
}

/// Every refusal writes its own response and yields null; the success arm hands
/// the committed revision and tag to the caller, which attaches the tag before
/// writing the response.
fn resolveOutcome(hx: Hx, fleet_id: []const u8, outcome: patch_txn.TxnOutcome) ?patch_txn.Updated {
    switch (outcome) {
        .updated => |u| return u,
        .stale_etag => |current| {
            log.info("patch_stale_etag", .{ .error_code = ec.ERR_AGENTSFLEET_SOURCE_STALE, .fleet_id = fleet_id, .req_id = hx.req_id });
            common.errorResponsePrecondition(hx.res, ec.ERR_AGENTSFLEET_SOURCE_STALE, ec.MSG_AGENTSFLEET_SOURCE_STALE, hx.req_id, current);
        },
        .not_found => hx.fail(ec.ERR_AGENTSFLEET_NOT_FOUND, ec.MSG_AGENTSFLEET_NOT_FOUND),
        .invalid_transition => hx.fail(ec.ERR_AGENTSFLEET_ALREADY_TERMINAL, "Status transition not allowed from current state"),
        .invalid_trigger_markdown, .invalid_source_markdown => hx.fail(ec.ERR_AGENTSFLEET_INVALID_CONFIG, ec.MSG_AGENTSFLEET_INVALID_CONFIG),
        .invalid_gate_condition => hx.fail(ec.ERR_APPROVAL_CONDITION_INVALID, ec.MSG_APPROVAL_CONDITION_INVALID),
        .invalid_required_tags => hx.fail(ec.ERR_INVALID_REQUEST, "required tags: max 32 tags, each 1..64 chars"),
        .name_mismatch => hx.fail(ec.ERR_AGENTSFLEET_NAME_MISMATCH, ec.MSG_AGENTSFLEET_NAME_MISMATCH),
        .lock_timeout => {
            log.warn("patch_lock_timeout", .{ .error_code = ec.ERR_INTERNAL_DB_UNAVAILABLE, .fleet_id = fleet_id, .req_id = hx.req_id });
            common.internalDbUnavailable(hx.res, hx.req_id);
        },
    }
    return null;
}
