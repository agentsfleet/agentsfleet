//! PATCH /v1/workspaces/{ws}/fleets/{id} — partial update of a fleet.
//!
//! Body fields (all optional, presence-based; empty body → 200 no-op):
//!   - `config_json`      — replace config_json blob directly.
//!   - `status`           — "active" | "stopped" | "killed"; drives FSM.
//!   - `trigger_markdown` — reparses; rewrites trigger_markdown + config_json + name.
//!   - `source_markdown`  — reparses; validates name match; rewrites source_markdown.
//!
//! `config_json` and `trigger_markdown` are mutually exclusive (both drive
//! `core.fleets.config_json`).
//!
//! All paths run inside one transaction with `SELECT … FOR UPDATE`
//! row-lock so the cross-file name invariant observes locked state. The
//! txn locks exactly one `core.fleets` row → deadlock structurally
//! impossible (Invariant 10). Per-txn defensive timeouts: lock_timeout=5s,
//! statement_timeout=10s, idle_in_transaction_session_timeout=5s. PG
//! `55P03` (lock_timeout) → 503 ERR_INTERNAL_DB_UNAVAILABLE.
//!
//! `updated_at` (BIGINT ms epoch) doubles as config_revision — monotonic.

const std = @import("std");
const clock = @import("common").clock;
const httpz = @import("httpz");
const pg = @import("pg");
const logging = @import("log");

const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const fleet_config = @import("../../../fleet_runtime/config.zig");
const create = @import("create.zig");
const cron_sync = @import("cron_sync.zig");
const workspace_guards = @import("../../workspace_guards.zig");

const log = logging.scoped(.fleet_api);

const Hx = hx_mod.Hx;

const PatchBody = struct {
    config_json: ?[]const u8 = null,
    status: ?[]const u8 = null,
    trigger_markdown: ?[]const u8 = null,
    source_markdown: ?[]const u8 = null,
};

// All deterministic outcomes of a body-field PATCH. Unexpected DB errors
// propagate via `anyerror`; everything the handler maps to a specific
// HTTP response lives here. Lowercase tags by intent — only consumed by
// innerPatchFleet below; not external API surface.
const TxnOutcome = union(enum) {
    updated: i64, // new updated_at (revision)
    not_found,
    invalid_transition,
    invalid_trigger_markdown,
    invalid_gate_condition,
    invalid_source_markdown,
    invalid_required_tags, // SKILL.md `tags:` outside placement bounds (UZ-REQ-001)
    name_mismatch,
    lock_timeout,
};

/// Partial update of a fleet. Validates inputs, persists in one SQL UPDATE
/// gated by the FSM, and publishes one control-stream signal per dirty
/// surface. Returns 404 if the fleet is missing or already-killed (a
/// killed fleet is a tombstone — no further state changes apply). Returns
/// 409 if the requested status transition is not allowed from the current
/// state (e.g. resume on an active fleet, stop on a stopped fleet).
pub fn innerPatchFleet(hx: Hx, req: *httpz.Request, workspace_id: []const u8, fleet_id: []const u8) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    if (!id_format.isSupportedWorkspaceId(fleet_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "fleet_id must be a valid UUIDv7");
        return;
    }

    const body = parsePatchBody(hx, req) orelse return;
    if (body.config_json == null and body.status == null and
        body.trigger_markdown == null and body.source_markdown == null)
    {
        hx.ok(.ok, .{ .fleet_id = fleet_id, .config_revision = @as(?i64, null) });
        return;
    }
    if (!validateBody(hx, body)) return;

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

    const outcome = patchFleetInTxn(hx.alloc, conn, workspace_id, fleet_id, body) catch |err| {
        log.err("patch_db_failed", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err), .fleet_id = fleet_id, .req_id = hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };

    const revision = switch (outcome) {
        .updated => |r| r,
        .not_found => return hx.fail(ec.ERR_AGENTSFLEET_NOT_FOUND, ec.MSG_AGENTSFLEET_NOT_FOUND),
        .invalid_transition => return hx.fail(ec.ERR_AGENTSFLEET_ALREADY_TERMINAL, "Status transition not allowed from current state"),
        .invalid_trigger_markdown, .invalid_source_markdown => return hx.fail(ec.ERR_AGENTSFLEET_INVALID_CONFIG, ec.MSG_AGENTSFLEET_INVALID_CONFIG),
        .invalid_gate_condition => return hx.fail(ec.ERR_APPROVAL_CONDITION_INVALID, ec.MSG_APPROVAL_CONDITION_INVALID),
        .invalid_required_tags => return hx.fail(ec.ERR_INVALID_REQUEST, "required tags: max 32 tags, each 1..64 chars"),
        .name_mismatch => return hx.fail(ec.ERR_AGENTSFLEET_NAME_MISMATCH, ec.MSG_AGENTSFLEET_NAME_MISMATCH),
        .lock_timeout => {
            log.warn("patch_lock_timeout", .{ .error_code = ec.ERR_INTERNAL_DB_UNAVAILABLE, .fleet_id = fleet_id, .req_id = hx.req_id });
            common.internalDbUnavailable(hx.res, hx.req_id);
            return;
        },
    };

    if (body.status != null or body.trigger_markdown != null or body.config_json != null) {
        const cron_result = cron_sync.syncStoredFleet(hx, workspace_id, fleet_id);
        if (cron_result != .ok and cron_result != .skipped) {
            _ = cron_sync.writeFailure(hx, cron_result);
            return;
        }
    }

    log.debug("patched", .{ .id = fleet_id, .workspace = workspace_id, .revision = revision, .status_set = body.status });
    if (body.status) |s| {
        hx.ok(.ok, .{ .fleet_id = fleet_id, .status = s, .config_revision = revision });
    } else {
        hx.ok(.ok, .{ .fleet_id = fleet_id, .config_revision = revision });
    }
}

fn parsePatchBody(hx: Hx, req: *httpz.Request) ?PatchBody {
    const body = req.body() orelse return .{};
    if (body.len == 0) return .{};
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return null;
    const parsed = std.json.parseFromSlice(PatchBody, hx.alloc, body, .{ .ignore_unknown_fields = true }) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_MALFORMED_JSON);
        return null;
    };
    return parsed.value;
}

fn validateBody(hx: Hx, body: PatchBody) bool {
    // config_json and trigger_markdown both drive core.fleets.config_json;
    // sending both is ambiguous — reject at the door.
    if (body.config_json != null and body.trigger_markdown != null) {
        hx.fail(ec.ERR_INVALID_REQUEST, "config_json and trigger_markdown are mutually exclusive");
        return false;
    }
    if (body.config_json) |cj| {
        if (cj.len == 0) {
            hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_AGENTSFLEET_CONFIG_REQUIRED);
            return false;
        }
    }
    // Same 1..64KiB cap as create (`create.MAX_*_LEN`): an edit cannot smuggle an
    // oversized body past the create-time guard. Load-bearing now that the
    // `SKILL.md` body rides every lease — an unbounded source_markdown would
    // inflate every lease toward MAX_LEASE_BYTES and the model context.
    if (body.trigger_markdown) |tm| if (tm.len == 0 or tm.len > create.MAX_TRIGGER_LEN) {
        hx.fail(ec.ERR_INVALID_REQUEST, "trigger_markdown must be 1..64KiB");
        return false;
    };
    if (body.source_markdown) |sm| if (sm.len == 0 or sm.len > create.MAX_SOURCE_LEN) {
        hx.fail(ec.ERR_INVALID_REQUEST, "source_markdown must be 1..64KiB");
        return false;
    };
    if (body.status) |s| {
        // Only operator-targetable states. `paused` is reserved for the
        // platform's anomaly gate and rejected here so callers can't use
        // PATCH to forge a system-halt provenance.
        const allowed =
            std.mem.eql(u8, s, fleet_config.FleetStatus.active.toSlice()) or
            std.mem.eql(u8, s, fleet_config.FleetStatus.stopped.toSlice()) or
            std.mem.eql(u8, s, fleet_config.FleetStatus.killed.toSlice());
        if (!allowed) {
            hx.fail(ec.ERR_INVALID_REQUEST, "status must be one of \"active\", \"stopped\", \"killed\"");
            return false;
        }
    }
    return true;
}

/// SELECT FOR UPDATE → reparse → UPDATE gated by FSM. See file header
/// for FSM table, timeouts, and Invariant 10 (single-row lock).
fn patchFleetInTxn(
    alloc: std.mem.Allocator,
    conn: *pg.Conn,
    workspace_id: []const u8,
    fleet_id: []const u8,
    body: PatchBody,
) anyerror!TxnOutcome {
    _ = try conn.exec("BEGIN", .{});
    var tx_open = true;
    defer if (tx_open) {
        conn.rollback() catch |err| log.warn("rollback_fail", .{ .error_code = ec.ERR_INTERNAL_OPERATION_FAILED, .err = @errorName(err) });
    };

    _ = conn.exec("SET LOCAL lock_timeout = '5s'", .{}) catch |err| return mapPgErr(conn, err);
    _ = conn.exec("SET LOCAL statement_timeout = '10s'", .{}) catch |err| return mapPgErr(conn, err);
    _ = conn.exec("SET LOCAL idle_in_transaction_session_timeout = '5s'", .{}) catch |err| return mapPgErr(conn, err);

    // SELECT FOR UPDATE — acquire row-lock + snapshot (name, status). Both
    // are needed: name for source_md cross-file invariant; status for
    // distinguishing 404 (terminal/killed) from 409 (FSM rejects) when the
    // UPDATE returns 0 rows.
    const current = blk: {
        var sel_q = PgQuery.from(conn.query(
            \\SELECT name, status FROM core.fleets
            \\WHERE id = $1::uuid AND workspace_id = $2::uuid
            \\FOR UPDATE
        , .{ fleet_id, workspace_id }) catch |err| return mapPgErr(conn, err));
        defer sel_q.deinit();
        const row = try sel_q.next();
        if (row == null) break :blk null;
        const n = try alloc.dupe(u8, try row.?.get([]const u8, 0));
        const s = try alloc.dupe(u8, try row.?.get([]const u8, 1));
        break :blk .{ .name = n, .status = s };
    };
    if (current == null) return .{ .not_found = {} };
    defer alloc.free(current.?.name);
    defer alloc.free(current.?.status);

    // Reparse body markdown if present. ParsedTrigger / SkillMetadata own
    // heap; deinit'd on every return path (success and error).
    var parsed_trigger: ?fleet_config.ParsedTrigger = null;
    defer if (parsed_trigger) |*pt| pt.deinit(alloc);
    if (body.trigger_markdown) |tm| {
        parsed_trigger = fleet_config.parseTriggerMarkdownWithJson(alloc, tm) catch return .{ .invalid_trigger_markdown = {} };
    }

    var skill_meta: ?fleet_config.SkillMetadata = null;
    defer if (skill_meta) |*sm| sm.deinit(alloc);
    // Re-derived placement tags when source_markdown is reparsed; NULL ⇒ the
    // UPDATE's COALESCE keeps the existing required_tags. Borrowed from skill_meta
    // (alive for the txn) and passed straight through as a TEXT[] param.
    var new_required_tags: ?[]const []const u8 = null;
    if (body.source_markdown) |sm| {
        skill_meta = fleet_config.parseSkillMetadata(alloc, sm) catch return .{ .invalid_source_markdown = {} };
        const target_name = if (parsed_trigger) |pt| pt.config.name else current.?.name;
        if (!std.mem.eql(u8, skill_meta.?.name, target_name)) return .{ .name_mismatch = {} };
        if (!fleet_config.validRequiredTags(skill_meta.?.tags)) return .{ .invalid_required_tags = {} };
        new_required_tags = skill_meta.?.tags;
    }

    const new_config_json: ?[]const u8 = if (parsed_trigger) |pt| pt.config_json else body.config_json;

    // UZ-APPROVAL-005: reject a malformed gate condition on the to-be-persisted
    // config_json (markdown or direct path); lenient parse, non-gate fall-through.
    if (new_config_json) |cj| {
        if (fleet_config.parseFleetConfig(alloc, cj)) |cfg| {
            defer cfg.deinit(alloc);
            if (cfg.gates) |g| if (fleet_config.firstInvalidGateCondition(g.rules) != null)
                return .{ .invalid_gate_condition = {} };
        } else |err| if (err == error.OutOfMemory) return err;
    }

    const new_name: ?[]const u8 = if (parsed_trigger) |pt| pt.config.name else null;

    const now_ms = clock.nowMillis();
    const active = fleet_config.FleetStatus.active.toSlice();
    const stopped = fleet_config.FleetStatus.stopped.toSlice();
    const killed = fleet_config.FleetStatus.killed.toSlice();
    const paused = fleet_config.FleetStatus.paused.toSlice();

    const revision_opt: ?i64 = blk: {
        var upd_q = PgQuery.from(conn.query(
            \\UPDATE core.fleets SET
            \\    config_json      = COALESCE($1::jsonb, config_json),
            \\    status           = COALESCE($2,        status),
            \\    trigger_markdown = COALESCE($11,       trigger_markdown),
            \\    source_markdown  = COALESCE($12,       source_markdown),
            \\    name             = COALESCE($13,       name),
            \\    required_tags    = COALESCE($14::text[], required_tags),
            \\    updated_at       = $3
            \\WHERE id = $4::uuid
            \\  AND workspace_id = $5::uuid
            \\  AND status != $6
            \\  AND (
            \\        $2::text IS NULL
            \\     OR ($2 = $6)
            \\     OR ($2 = $7 AND status = ANY($9::text[]))
            \\     OR ($2 = $8 AND status = ANY($10::text[]))
            \\  )
            \\RETURNING updated_at
        , .{
            new_config_json,
            body.status,
            now_ms,
            fleet_id,
            workspace_id,
            killed,
            stopped,
            active,
            &[_][]const u8{ active, paused },
            &[_][]const u8{ stopped, paused },
            body.trigger_markdown,
            body.source_markdown,
            new_name,
            new_required_tags,
        }) catch |err| return mapPgErr(conn, err));
        defer upd_q.deinit();
        if (try upd_q.next()) |row| break :blk try row.get(i64, 0);
        break :blk null;
    };

    if (revision_opt == null) {
        // FSM/terminal guard rejected. Use the snapshot we already hold to
        // distinguish — no extra round-trip needed (we still own the lock).
        if (std.mem.eql(u8, current.?.status, killed)) return .{ .not_found = {} };
        return .{ .invalid_transition = {} };
    }

    _ = try conn.exec("COMMIT", .{});
    tx_open = false;
    return .{ .updated = revision_opt.? };
}

/// Map a pg driver error to a TxnOutcome when the SQLSTATE is one the
/// handler surfaces as a deterministic outcome (e.g. lock_timeout 55P03 →
/// 503 retryable). Otherwise propagate the raw error so the caller's
/// `catch` falls into the generic 500 path. The driver carries the
/// SQLSTATE on `conn.err.?.code` after `error.PG`.
fn mapPgErr(conn: *pg.Conn, err: anyerror) anyerror!TxnOutcome {
    if (err == error.PG) {
        if (conn.err) |pg_err| {
            if (std.mem.eql(u8, pg_err.code, "55P03")) return .{ .lock_timeout = {} };
        }
    }
    return err;
}
