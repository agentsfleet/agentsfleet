const std = @import("std");
const httpz = @import("httpz");
const logging = @import("log");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const importer = @import("../../../fleet_bundle/importer.zig");
const store = @import("../../../fleet_bundle/store.zig");
const resolve = @import("resolve.zig");
const clock = @import("common").clock;

const log = logging.scoped(.fleet_bundle_api);

/// Concurrent Fleet Bundle import ceiling. Bounds outbound GitHub fetches (whose
/// body read has no std deadline) to a fixed slot count, so a stalled upstream
/// holds at most one slot — never the general request pool.
const MAX_CONCURRENT_IMPORTS: u32 = 12;
var in_flight_imports: std.atomic.Value(u32) = .init(0);

const SupportFileSummary = struct {
    path: []const u8,
    size_bytes: usize,
};

/// POST /v1/workspaces/{workspace_id}/fleets/bundles/snapshots — import + validate
/// a Fleet Bundle (paste markdown, or a GitHub/template source fetched
/// server-side), store the canonical snapshot in R2 when it carries support
/// files, persist metadata in Postgres, and return the requirement preview.
pub fn innerImport(hx: hx_mod.Hx, req: *httpz.Request, workspace_id: []const u8) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }

    const prior = in_flight_imports.fetchAdd(1, .acq_rel);
    defer _ = in_flight_imports.fetchSub(1, .acq_rel);
    if (prior >= MAX_CONCURRENT_IMPORTS) {
        tooManyImports(hx);
        return;
    }

    const parsed = parseRequest(hx, req) orelse return;
    defer parsed.deinit();
    if (!authorizeWs(hx, workspace_id)) return;

    var resolved = resolve.resolve(hx.alloc, hx.ctx.io, parsed.value) catch |err| {
        failImport(hx, err);
        return;
    };
    defer resolved.deinit(hx.alloc);

    const prepared = importer.prepare(hx.alloc, resolved.body) catch |err| {
        failImport(hx, err);
        return;
    };
    defer prepared.deinit(hx.alloc);

    const stored_id = persist(hx, workspace_id, &resolved, prepared) orelse return;
    defer hx.alloc.free(stored_id);
    respond(hx, resolved.body, prepared, stored_id);
}

fn parseRequest(hx: hx_mod.Hx, req: *httpz.Request) ?std.json.Parsed(resolve.ImportRequest) {
    const body = req.body() orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_BODY_REQUIRED);
        return null;
    };
    if (!common.checkBodySize(req, hx.res, body, hx.req_id)) return null;
    return std.json.parseFromSlice(resolve.ImportRequest, hx.alloc, body, .{ .ignore_unknown_fields = true }) catch {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_MALFORMED_JSON);
        return null;
    };
}

fn authorizeWs(hx: hx_mod.Hx, workspace_id: []const u8) bool {
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return false;
    };
    defer hx.ctx.pool.release(conn);
    if (!common.authorizeWorkspace(conn, hx.principal, workspace_id)) {
        hx.fail(ec.ERR_FORBIDDEN, "Workspace access denied");
        return false;
    }
    return true;
}

/// Store the snapshot (R2 first, then Postgres). The R2 put runs only when the
/// bundle carries support files — paste/skill-only bundles need no object store,
/// so a workspace with R2 unset still imports them. Returns the stored bundle id.
fn persist(
    hx: hx_mod.Hx,
    workspace_id: []const u8,
    resolved: *const resolve.Resolved,
    prepared: importer.PreparedBundle,
) ?[]const u8 {
    if (resolved.body.support_files.len > 0) {
        if (!putSnapshot(hx, resolved, prepared)) return null;
    }
    return insertBundle(hx, workspace_id, resolved.body, prepared);
}

fn putSnapshot(hx: hx_mod.Hx, resolved: *const resolve.Resolved, prepared: importer.PreparedBundle) bool {
    const r2 = hx.ctx.r2 orelse {
        storageUnavailable(hx);
        return false;
    };
    const fb = if (resolved.fetched) |*f| f else {
        common.internalOperationError(hx.res, "bundle has support files but no fetched content", hx.req_id);
        return false;
    };
    const canonical = fb.canonicalTar(hx.alloc) catch {
        common.internalOperationError(hx.res, "bundle canonicalization failed", hx.req_id);
        return false;
    };
    defer hx.alloc.free(canonical);
    r2.put(prepared.snapshot_key, canonical) catch {
        storageUnavailable(hx);
        return false;
    };
    return true;
}

fn insertBundle(
    hx: hx_mod.Hx,
    workspace_id: []const u8,
    body: importer.ImportBody,
    prepared: importer.PreparedBundle,
) ?[]const u8 {
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return null;
    };
    defer hx.ctx.pool.release(conn);
    const bundle_id = id_format.generateFleetBundleId(hx.alloc) catch {
        common.internalOperationError(hx.res, "identifier generation failed", hx.req_id);
        return null;
    };
    defer hx.alloc.free(bundle_id);
    return store.insertOrFetchId(conn, hx.alloc, .{
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
        .now_ms = clock.nowMillis(),
    }) catch |err| {
        log.err("import_store_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err), .req_id = hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return null;
    };
}

fn respond(hx: hx_mod.Hx, body: importer.ImportBody, prepared: importer.PreparedBundle, stored_id: []const u8) void {
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

fn parseJsonValue(hx: hx_mod.Hx, json: []const u8) ?std.json.Parsed(std.json.Value) {
    return std.json.parseFromSlice(std.json.Value, hx.alloc, json, .{}) catch {
        common.internalOperationError(hx.res, "bundle preview serialization failed", hx.req_id);
        return null;
    };
}

/// Shared by the import response and the bundle detail handler (`get.zig`).
pub fn supportSummaries(alloc: std.mem.Allocator, files: []const importer.SupportFile) ![]SupportFileSummary {
    const summaries = try alloc.alloc(SupportFileSummary, files.len);
    for (files, 0..) |file, i| {
        summaries[i] = .{ .path = file.path, .size_bytes = file.content.len };
    }
    return summaries;
}

fn tooManyImports(hx: hx_mod.Hx) void {
    hx.res.header(common.HEADER_RETRY_AFTER, common.RETRY_AFTER_BRIEF_VALUE);
    common.errorResponse(hx.res, ec.ERR_FLEET_BUNDLE_TOO_MANY_IMPORTS, "Too many Fleet Bundle imports in flight", hx.req_id);
}

fn storageUnavailable(hx: hx_mod.Hx) void {
    common.errorResponse(hx.res, ec.ERR_FLEET_BUNDLE_STORAGE_UNAVAILABLE, "Fleet Bundle snapshot storage is unavailable", hx.req_id);
}

fn failImport(hx: hx_mod.Hx, err: anyerror) void {
    switch (err) {
        error.MissingSkill => hx.fail(ec.ERR_FLEET_BUNDLE_INVALID, "missing_skill"),
        error.UploadAttachmentsUnsupported => hx.fail(ec.ERR_FLEET_BUNDLE_INVALID, "upload sources cannot carry support files; use a github or template source"),
        error.InvalidSourceRef => hx.fail(ec.ERR_FLEET_BUNDLE_INVALID, "source_ref must be 'owner/repo' for a github source"),
        error.InvalidSource => hx.fail(ec.ERR_FLEET_BUNDLE_INVALID, "source reference is not a valid GitHub owner/repo/ref"),
        error.InvalidSourceKind => hx.fail(ec.ERR_FLEET_BUNDLE_INVALID, "source_kind must be template, upload, or github"),
        error.InvalidSkill => hx.fail(ec.ERR_FLEET_BUNDLE_INVALID, "SKILL.md frontmatter is invalid"),
        error.InvalidTrigger => hx.fail(ec.ERR_FLEET_BUNDLE_INVALID, "TRIGGER.md frontmatter is invalid"),
        error.NameMismatch => hx.fail(ec.ERR_AGENTSFLEET_NAME_MISMATCH, ec.MSG_AGENTSFLEET_NAME_MISMATCH),
        error.UnsafePath => hx.fail(ec.ERR_FLEET_BUNDLE_INVALID, "unsafe_path"),
        error.SecretShape => hx.fail(ec.ERR_FLEET_BUNDLE_INVALID, "support files must not carry credential-shaped content"),
        error.TooLarge => common.errorResponse(hx.res, ec.ERR_PAYLOAD_TOO_LARGE, "Fleet Bundle exceeds a configured size cap", hx.req_id),
        error.TarballTooLarge => common.errorResponse(hx.res, ec.ERR_PAYLOAD_TOO_LARGE, "fetched Fleet Bundle exceeds the snapshot size cap", hx.req_id),
        error.TooManyFiles => common.errorResponse(hx.res, ec.ERR_PAYLOAD_TOO_LARGE, "fetched Fleet Bundle exceeds the file-count cap", hx.req_id),
        error.FetchFailed, error.InvalidUrl => common.errorResponse(hx.res, ec.ERR_FLEET_BUNDLE_FETCH_FAILED, "the Fleet Bundle source could not be fetched from GitHub", hx.req_id),
        error.DisallowedRedirect => common.errorResponse(hx.res, ec.ERR_FLEET_BUNDLE_FETCH_FAILED, "the GitHub source redirected to a disallowed host", hx.req_id),
        error.CorruptArchive => common.errorResponse(hx.res, ec.ERR_FLEET_BUNDLE_FETCH_FAILED, "the fetched Fleet Bundle archive could not be read", hx.req_id),
        error.OutOfMemory => common.internalOperationError(hx.res, "bundle import allocation failed", hx.req_id),
        else => common.internalOperationError(hx.res, "bundle import failed", hx.req_id),
    }
}
