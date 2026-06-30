const std = @import("std");
const httpz = @import("httpz");
const logging = @import("log");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const importer = @import("../../../fleet_bundle/importer.zig");
const store = @import("../../../fleet_bundle/store.zig");
const imports_h = @import("imports.zig");

const log = logging.scoped(.fleet_bundle_api);

pub fn innerGet(hx: hx_mod.Hx, req: *httpz.Request, workspace_id: []const u8, bundle_id: []const u8) void {
    _ = req;
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    if (!common.requireUuidV7Id(hx.res, hx.req_id, bundle_id, "bundle_id")) return;

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return;
    }

    const detail = store.fetchDetail(conn, hx.alloc, workspace_id, bundle_id) catch |err| {
        log.err("fetch_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err), .req_id = hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    } orelse {
        hx.fail(ec.ERR_FLEET_BUNDLE_NOT_FOUND, "Fleet Bundle not found");
        return;
    };
    defer detail.deinit(hx.alloc);

    const requirements = parseJsonValue(hx, detail.requirements_json) orelse return;
    defer requirements.deinit();
    const manifest = std.json.parseFromSlice([]importer.SupportFileManifest, hx.alloc, detail.support_files_json, .{}) catch {
        common.internalOperationError(hx.res, "bundle support serialization failed", hx.req_id);
        return;
    };
    defer manifest.deinit();
    const summaries = imports_h.manifestSummaries(hx.alloc, manifest.value) catch {
        common.internalOperationError(hx.res, "support summary allocation failed", hx.req_id);
        return;
    };
    defer hx.alloc.free(summaries);

    hx.ok(.ok, .{
        .bundle_id = detail.id,
        .name = detail.name,
        .source_kind = detail.source_kind,
        .source_ref = detail.source_ref,
        .visibility = detail.visibility,
        .validation_status = detail.validation_status,
        .content_hash = detail.content_hash,
        .requirements = requirements.value,
        .support_files = summaries,
        .created_at = detail.created_at,
        .updated_at = detail.updated_at,
    });
}

fn parseJsonValue(hx: hx_mod.Hx, json: []const u8) ?std.json.Parsed(std.json.Value) {
    return std.json.parseFromSlice(std.json.Value, hx.alloc, json, .{}) catch {
        common.internalOperationError(hx.res, "bundle requirements serialization failed", hx.req_id);
        return null;
    };
}
