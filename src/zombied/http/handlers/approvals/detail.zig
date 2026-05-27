//! GET /v1/workspaces/{ws}/approvals/{gate_id} — single approval-gate read.
//!
//! Drives the dashboard detail page. 404 when the gate doesn't exist OR
//! belongs to a different workspace; cross-tenant lookup leaks no info
//! beyond "not found".

const std = @import("std");
const httpz = @import("httpz");
const logging = @import("log");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const approval_gate_db = @import("../../../zombie/approval_gate_db.zig");

const log = logging.scoped(.http_approvals_detail);

pub fn innerGetApproval(
    hx: hx_mod.Hx,
    req: *httpz.Request,
    workspace_id: []const u8,
    gate_id: []const u8,
) void {
    _ = req;
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    if (!id_format.isSupportedWorkspaceId(gate_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "gate_id must be a UUIDv7");
        return;
    }

    // Authz scope: hold conn only for the workspace check, then release so
    // downstream pool-acquiring calls (getByGateId) don't compete for slots
    // with this handler's own held connection on small test pools.
    {
        const conn = hx.ctx.pool.acquire() catch {
            common.internalDbUnavailable(hx.res, hx.req_id);
            return;
        };
        defer hx.ctx.pool.release(conn);
        if (!common.authorizeWorkspaceAndSetTenantContext(conn, hx.principal, workspace_id)) {
            hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
            return;
        }
    }

    const maybe_row = approval_gate_db.getByGateId(hx.ctx.pool, hx.alloc, gate_id, workspace_id) catch |err| {
        log.err("detail_failed", .{
            .error_code = ec.ERR_INTERNAL_DB_QUERY,
            .err = @errorName(err),
            .gate_id = gate_id,
        });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    var row = maybe_row orelse {
        hx.fail(ec.ERR_APPROVAL_NOT_FOUND, ec.MSG_APPROVAL_NOT_FOUND);
        return;
    };
    defer row.deinit(hx.alloc);

    const parsed_evidence = std.json.parseFromSlice(std.json.Value, hx.alloc, row.evidence_json, .{ .ignore_unknown_fields = true }) catch null;
    defer if (parsed_evidence) |p| p.deinit();

    var empty_obj = std.json.ObjectMap.init(hx.alloc);
    defer empty_obj.deinit();
    const evidence_value: std.json.Value = if (parsed_evidence) |p| p.value else .{ .object = empty_obj };

    hx.ok(.ok, .{
        .gate_id = row.gate_id,
        .zombie_id = row.zombie_id,
        .zombie_name = row.zombie_name,
        .workspace_id = row.workspace_id,
        .action_id = row.action_id,
        .tool_name = row.tool_name,
        .action_name = row.action_name,
        .gate_kind = row.gate_kind,
        .proposed_action = row.proposed_action,
        .blast_radius = row.blast_radius,
        .status = row.status,
        .detail = row.detail,
        .requested_at = row.requested_at,
        .timeout_at = row.timeout_at,
        .updated_at = row.updated_at,
        .resolved_by = row.resolved_by,
        .evidence = evidence_value,
    });
}
