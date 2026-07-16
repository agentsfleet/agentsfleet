//! GET /v1/workspaces/{ws}/fleets/{id} — single-fleet detail read.
//!
//! Serializes the full editable surface (`source_markdown`, nullable
//! `trigger_markdown`) plus `bundle_content_hash`, the trigger list projected
//! from `config_json`, and the same lifetime aggregates the list row carries
//! (`events_processed`, `budget_used_nanos` — server truth, never client
//! arithmetic). The response carries an `ETag` header over the editable
//! markdown; the source editor sends it back as `If-Match` on PATCH.
//!
//! A fleet id that exists in a different workspace answers 404, never 403 —
//! the workspace-scoped WHERE finds no row, so existence is not leaked.

const std = @import("std");
const pg = @import("pg");
const logging = @import("log");

const PgQuery = @import("../../../db/pg_query.zig").PgQuery;
const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const etag = @import("../../etag.zig");
const sql = @import("sql.zig");

const log = logging.scoped(.fleet_api);

const Hx = hx_mod.Hx;

/// Both ETag-attach failures (compute + header) answer the same 500: the read
/// cannot serve a tagless fleet, because an editor that gets no ETag cannot
/// send If-Match and its next save silently becomes last-write-wins.
const S_ETAG_ATTACH_FAILED = "Failed to load this fleet's source";

/// The fleet's editable surface — the ordered field list its ETag hashes.
/// Only the source pair: a lifecycle PATCH (stop/resume/kill) leaves it
/// unchanged, so an open editor with no source conflict is never 412'd. Read
/// here and by the PATCH txn (`patch_txn.zig`), so the tag both compute is
/// identical by construction.
pub fn sourceSurface(source_markdown: []const u8, trigger_markdown: ?[]const u8) [2]?[]const u8 {
    return .{ source_markdown, trigger_markdown };
}

/// Wire shape of the detail response — the list row's fields plus the
/// editable markdown pair and the bundle pin. Nullable columns serialize as
/// JSON null (the editor renders "no TRIGGER.md" from it).
const FleetDetail = struct {
    id: []const u8,
    name: []const u8,
    status: []const u8,
    source_markdown: []const u8,
    trigger_markdown: ?[]const u8,
    bundle_content_hash: ?[]const u8,
    triggers: ?std.json.Value,
    events_processed: i64,
    budget_used_nanos: i64,
    created_at: i64,
    updated_at: i64,
};

pub fn innerGetFleet(hx: Hx, workspace_id: []const u8, fleet_id: []const u8) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    if (!id_format.isSupportedWorkspaceId(fleet_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, "fleet_id must be a valid UUIDv7");
        return;
    }

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspaceAndSetTenantContext(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return;
    }

    var q = PgQuery.from(conn.query(sql.SELECT_FLEET_DETAIL, .{ fleet_id, workspace_id }) catch |err| {
        log.err("get_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err), .req_id = hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    });
    defer q.deinit();

    const row = (q.next() catch {
        common.internalDbError(hx.res, hx.req_id);
        return;
    }) orelse {
        // Missing id and cross-workspace id are indistinguishable on purpose.
        log.debug("get_not_found", .{ .error_code = ec.ERR_AGENTSFLEET_NOT_FOUND, .req_id = hx.req_id });
        hx.fail(ec.ERR_AGENTSFLEET_NOT_FOUND, ec.MSG_AGENTSFLEET_NOT_FOUND);
        return;
    };

    writeDetail(hx, row);
}

/// Serializes one detail row. Row slices are borrowed — `hx.ok` writes the
/// JSON into the response buffer before this returns, so nothing outlives
/// the open query result.
fn writeDetail(hx: Hx, row: pg.Row) void {
    const detail = readDetail(hx.alloc, row) catch {
        log.err("get_row_read_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .req_id = hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    defer if (detail.parsed_triggers) |p| p.deinit();

    const tag = etag.compute(hx.alloc, &sourceSurface(detail.body.source_markdown, detail.body.trigger_markdown)) catch {
        common.internalOperationError(hx.res, S_ETAG_ATTACH_FAILED, hx.req_id);
        return;
    };
    etag.attach(hx.res, tag) catch {
        common.internalOperationError(hx.res, S_ETAG_ATTACH_FAILED, hx.req_id);
        return;
    };

    hx.ok(.ok, detail.body);
}

const ReadDetail = struct {
    body: FleetDetail,
    parsed_triggers: ?std.json.Parsed(std.json.Value),
};

fn readDetail(alloc: std.mem.Allocator, row: pg.Row) !ReadDetail {
    const triggers_raw = try row.get(?[]const u8, 6);
    const parsed_triggers: ?std.json.Parsed(std.json.Value) = if (triggers_raw) |raw|
        std.json.parseFromSlice(std.json.Value, alloc, raw, .{}) catch null
    else
        null;

    return .{
        .parsed_triggers = parsed_triggers,
        .body = .{
            .id = try row.get([]const u8, 0),
            .name = try row.get([]const u8, 1),
            .status = try row.get([]const u8, 2),
            .source_markdown = try row.get([]const u8, 3),
            .trigger_markdown = try row.get(?[]const u8, 4),
            .bundle_content_hash = try row.get(?[]const u8, 5),
            .triggers = if (parsed_triggers) |p| p.value else null,
            .events_processed = try row.get(i64, 7),
            .budget_used_nanos = try row.get(i64, 8),
            .created_at = try row.get(i64, 9),
            .updated_at = try row.get(i64, 10),
        },
    };
}
