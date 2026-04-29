//! GET /v1/workspaces/{ws}/approvals — workspace-scoped approval-gate inbox.
//!
//! Returns pending gates oldest-first (oldest is most urgent). Optional
//! filters: zombie_id, gate_kind, status. Cursor pagination over
//! (requested_at, id) so concurrent inserts don't cause silent skips.

const std = @import("std");
const httpz = @import("httpz");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const approval_gate_db = @import("../../../zombie/approval_gate_db.zig");
const keyset_cursor = @import("../../../zombie/keyset_cursor.zig");

const log = std.log.scoped(.http_approvals_list);

const LIMIT_DEFAULT: u32 = 50;
const LIMIT_MAX: u32 = 200;

pub fn innerListApprovals(hx: hx_mod.Hx, req: *httpz.Request, workspace_id: []const u8) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }

    const qs = req.query() catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "malformed query string");
        return;
    };

    const limit = parseLimit(qs) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "limit must be between 1 and 200");
        return;
    };

    const cursor: ?keyset_cursor.Cursor = if (qs.get("cursor")) |raw| (keyset_cursor.parse(raw) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, "invalid cursor");
        return;
    }) else null;

    // Authz scope: hold conn only for the workspace check, then release so
    // listPending's pool.acquire() doesn't compete with this handler's own
    // held connection on small test pools.
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

    const filter = approval_gate_db.ListFilter{
        .workspace_id = workspace_id,
        .status = qs.get("status"),
        .zombie_id = qs.get("zombie_id"),
        .gate_kind = qs.get("gate_kind"),
    };

    var result = approval_gate_db.listPending(hx.ctx.pool, hx.alloc, filter, cursor, limit) catch |err| {
        log.err("approvals.list_failed err={s} workspace_id={s}", .{ @errorName(err), workspace_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    defer result.deinit(hx.alloc);

    writeResponse(hx, result.items, limit) catch |err| {
        log.err("approvals.list_response_failed err={s}", .{@errorName(err)});
        common.internalDbError(hx.res, hx.req_id);
    };
}

const ListItemJson = struct {
    gate_id: []const u8,
    zombie_id: []const u8,
    zombie_name: []const u8,
    workspace_id: []const u8,
    action_id: []const u8,
    tool_name: []const u8,
    action_name: []const u8,
    gate_kind: []const u8,
    proposed_action: []const u8,
    blast_radius: []const u8,
    status: []const u8,
    detail: []const u8,
    requested_at: i64,
    timeout_at: i64,
    updated_at: ?i64,
    resolved_by: []const u8,
    evidence: std.json.Value,
};

fn writeResponse(hx: hx_mod.Hx, rows: []approval_gate_db.PendingRow, limit: u32) !void {
    var items = try hx.alloc.alloc(ListItemJson, rows.len);
    defer hx.alloc.free(items);
    var parsed_evidence = try hx.alloc.alloc(?std.json.Parsed(std.json.Value), rows.len);
    defer {
        for (parsed_evidence) |maybe_p| if (maybe_p) |p| p.deinit();
        hx.alloc.free(parsed_evidence);
    }

    for (rows, 0..) |row, i| {
        const evidence_value = parseEvidence(hx.alloc, row.evidence_json) catch null;
        parsed_evidence[i] = evidence_value;
        items[i] = .{
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
            .evidence = if (evidence_value) |p| p.value else .{ .object = std.json.ObjectMap.init(hx.alloc) },
        };
    }

    const next_cursor: ?[]u8 = if (rows.len == limit and rows.len > 0) blk: {
        const last = rows[rows.len - 1];
        break :blk try keyset_cursor.format(hx.alloc, .{
            .created_at_ms = last.requested_at,
            .id = last.gate_id,
        });
    } else null;
    defer if (next_cursor) |nc| hx.alloc.free(nc);

    hx.ok(.ok, .{ .items = items, .next_cursor = next_cursor });
}

fn parseEvidence(alloc: std.mem.Allocator, json_str: []const u8) !std.json.Parsed(std.json.Value) {
    return std.json.parseFromSlice(std.json.Value, alloc, json_str, .{ .ignore_unknown_fields = true });
}

fn parseLimit(qs: anytype) !u32 {
    const raw = qs.get("limit") orelse return LIMIT_DEFAULT;
    const n = std.fmt.parseInt(u32, raw, 10) catch return error.InvalidLimit;
    if (n == 0 or n > LIMIT_MAX) return error.InvalidLimit;
    return n;
}
