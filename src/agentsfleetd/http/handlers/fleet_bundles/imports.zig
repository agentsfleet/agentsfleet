const std = @import("std");
const httpz = @import("httpz");
const logging = @import("log");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const importer = @import("../../../fleet_bundle/importer.zig");
const store = @import("../../../fleet_bundle/store.zig");
const clock = @import("common").clock;

const log = logging.scoped(.fleet_bundle_api);

const SupportFileSummary = struct {
    path: []const u8,
    size_bytes: usize,
};

pub fn innerImport(hx: hx_mod.Hx, req: *httpz.Request, workspace_id: []const u8) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    const body = parseBody(hx, req) orelse return;
    const prepared = importer.prepare(hx.alloc, body) catch |err| {
        failImport(hx, err);
        return;
    };
    defer prepared.deinit(hx.alloc);

    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return;
    };
    defer hx.ctx.pool.release(conn);

    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return;
    }

    const bundle_id = id_format.generateFleetBundleId(hx.alloc) catch {
        common.internalOperationError(hx.res, "identifier generation failed", hx.req_id);
        return;
    };
    defer hx.alloc.free(bundle_id);
    const now_ms = clock.nowMillis();
    const stored_id = store.insertOrFetchId(conn, hx.alloc, .{
        .id = bundle_id,
        .workspace_id = workspace_id,
        .name = prepared.name,
        .source_kind = body.source_kind,
        .source_ref = body.source_ref,
        .visibility = importer.VISIBILITY_WORKSPACE,
        .content_hash = prepared.content_hash,
        .snapshot_key = prepared.snapshot_key,
        .skill_markdown = body.skill_markdown,
        .trigger_markdown = body.trigger_markdown,
        .support_files_json = prepared.support_files_json,
        .requirements_json = prepared.requirements_json,
        .validation_status = importer.STATUS_VALID,
        .now_ms = now_ms,
    }) catch |err| {
        log.err("import_store_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err), .req_id = hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return;
    };
    defer hx.alloc.free(stored_id);

    const requirements = parseJsonValue(hx, prepared.requirements_json) orelse return;
    defer requirements.deinit();
    const summaries = supportSummaries(hx.alloc, body.support_files) catch {
        common.internalOperationError(hx.res, "support summary allocation failed", hx.req_id);
        return;
    };
    defer hx.alloc.free(summaries);

    hx.ok(.created, .{
        .bundle_id = stored_id,
        .name = prepared.name,
        .source_kind = body.source_kind,
        .source_ref = body.source_ref,
        .validation_status = importer.STATUS_VALID,
        .content_hash = prepared.content_hash,
        .snapshot_key = prepared.snapshot_key,
        .requirements = requirements.value,
        .support_files = summaries,
    });
}

fn parseBody(hx: hx_mod.Hx, req: *httpz.Request) ?importer.ImportBody {
    const body = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_BODY_REQUIRED);
        return null;
    };
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return null;
    const parsed = std.json.parseFromSlice(importer.ImportBody, hx.alloc, body, .{ .ignore_unknown_fields = true }) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_MALFORMED_JSON);
        return null;
    };
    return parsed.value;
}

fn parseJsonValue(hx: hx_mod.Hx, json: []const u8) ?std.json.Parsed(std.json.Value) {
    return std.json.parseFromSlice(std.json.Value, hx.alloc, json, .{}) catch {
        common.internalOperationError(hx.res, "bundle preview serialization failed", hx.req_id);
        return null;
    };
}

pub fn supportSummaries(alloc: std.mem.Allocator, files: []const importer.SupportFile) ![]SupportFileSummary {
    const summaries = try alloc.alloc(SupportFileSummary, files.len);
    for (files, 0..) |file, i| {
        summaries[i] = .{ .path = file.path, .size_bytes = file.content.len };
    }
    return summaries;
}

fn failImport(hx: hx_mod.Hx, err: anyerror) void {
    switch (err) {
        error.TooLarge => common.errorResponse(hx.res, ec.ERR_PAYLOAD_TOO_LARGE, "Fleet Bundle exceeds a configured size cap", hx.req_id),
        error.MissingSkill => hx.fail(ec.ERR_FLEET_BUNDLE_INVALID, "missing_skill"),
        error.InvalidSourceKind => hx.fail(ec.ERR_FLEET_BUNDLE_INVALID, "source_kind must be template, upload, or github"),
        error.InvalidSkill => hx.fail(ec.ERR_FLEET_BUNDLE_INVALID, "SKILL.md frontmatter is invalid"),
        error.InvalidTrigger => hx.fail(ec.ERR_FLEET_BUNDLE_INVALID, "TRIGGER.md frontmatter is invalid"),
        error.NameMismatch => hx.fail(ec.ERR_AGENTSFLEET_NAME_MISMATCH, ec.MSG_AGENTSFLEET_NAME_MISMATCH),
        error.UnsafePath => hx.fail(ec.ERR_FLEET_BUNDLE_INVALID, "unsafe_path"),
        error.SecretShape => hx.fail(ec.ERR_FLEET_BUNDLE_INVALID, "support files must not carry credential-shaped content"),
        error.OutOfMemory => common.internalOperationError(hx.res, "bundle import allocation failed", hx.req_id),
        else => common.internalOperationError(hx.res, "bundle import failed", hx.req_id),
    }
}
