//! Fleet template onboarding — the two scope-gated write paths (M103 §2):
//!   * POST /v1/admin/fleet-templates                     (platform tier)
//!   * POST /v1/workspaces/{workspace_id}/fleet-templates (tenant tier)
//!
//! Both run the same resolve → prepare → R2 → metadata pipeline as the bundle
//! importer, writing the canonical tar to Cloudflare R2 (R2) before any row is
//! committed (Dimension 2.3), then persisting a metadata-only catalog row. The
//! capability scope (`platform-template:write` / `template:write`) is enforced by
//! the requireScope middleware ahead of these handlers; the tenant path adds a
//! workspace-ownership check. Responses carry the tier in `visibility`
//! ("platform"/"tenant") and never an R2 key (Invariant 7).

const std = @import("std");
const httpz = @import("httpz");
const logging = @import("log");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const id_format = @import("../../../types/id_format.zig");
const importer = @import("../../../fleet_bundle/importer.zig");
const template_store = @import("../../../fleet_bundle/template_store.zig");
const resolve = @import("../fleet_bundles/resolve.zig");
const pipeline = @import("pipeline.zig");
const clock = @import("common").clock;

const Hx = hx_mod.Hx;
const log = logging.scoped(.fleet_template_api);

/// Parsed request + resolved source + prepared bundle for one onboarding call.
/// Owns all three; `deinit` frees prepared → resolved → parsed (prepared holds
/// only its own dupes, so the order among them is free).
const OnboardCtx = struct {
    parsed: std.json.Parsed(resolve.ImportRequest),
    resolved: resolve.Resolved,
    prepared: importer.PreparedBundle,

    fn deinit(self: *OnboardCtx, alloc: std.mem.Allocator) void {
        self.prepared.deinit(alloc);
        self.resolved.deinit(alloc);
        self.parsed.deinit();
    }
};

/// POST /v1/admin/fleet-templates — platform onboarding. The
/// `platform-template:write` scope is enforced upstream; there is no workspace.
pub fn innerPlatformOnboard(hx: Hx, req: *httpz.Request) void {
    var ctx = prepareOnboard(hx, req) orelse return;
    defer ctx.deinit(hx.alloc);
    const id = insertPlatform(hx, ctx.resolved.body, ctx.prepared) orelse return;
    defer hx.alloc.free(id);
    respond(hx, ctx.resolved.body, ctx.prepared, id, template_store.TIER_PLATFORM);
}

/// POST /v1/workspaces/{workspace_id}/fleet-templates — tenant onboarding. The
/// `template:write` scope is enforced upstream; ownership of the target workspace
/// is checked here, and the row is written only under that workspace_id.
pub fn innerTenantOnboard(hx: Hx, req: *httpz.Request, workspace_id: []const u8) void {
    if (!id_format.isSupportedWorkspaceId(workspace_id)) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_WORKSPACE_ID_REQUIRED);
        return;
    }
    if (!authorizeWs(hx, workspace_id)) return;
    var ctx = prepareOnboard(hx, req) orelse return;
    defer ctx.deinit(hx.alloc);
    const id = insertTenant(hx, workspace_id, ctx.resolved.body, ctx.prepared) orelse return;
    defer hx.alloc.free(id);
    respond(hx, ctx.resolved.body, ctx.prepared, id, template_store.TIER_TENANT);
}

/// Parse the request, fetch + validate the source, derive the content hash, and
/// (for bundles with support files) write the R2 snapshot — all before any
/// caller inserts a row. Returns null with the error already sent.
fn prepareOnboard(hx: Hx, req: *httpz.Request) ?OnboardCtx {
    const parsed = parseRequest(hx, req) orelse return null;
    errdefer parsed.deinit();

    var resolved = resolve.resolve(hx.alloc, hx.ctx.io, parsed.value) catch |err| {
        pipeline.failImport(hx, err);
        return null;
    };
    errdefer resolved.deinit(hx.alloc);

    const prepared = importer.prepare(hx.alloc, resolved.body) catch |err| {
        pipeline.failImport(hx, err);
        return null;
    };
    errdefer prepared.deinit(hx.alloc);

    if (resolved.body.support_files.len > 0) {
        if (!pipeline.putSnapshot(hx, &resolved, prepared)) return null;
    }
    return .{ .parsed = parsed, .resolved = resolved, .prepared = prepared };
}

fn parseRequest(hx: Hx, req: *httpz.Request) ?std.json.Parsed(resolve.ImportRequest) {
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

fn authorizeWs(hx: Hx, workspace_id: []const u8) bool {
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

fn insertPlatform(hx: Hx, body: importer.ImportBody, prepared: importer.PreparedBundle) ?[]const u8 {
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return null;
    };
    defer hx.ctx.pool.release(conn);
    return template_store.insertOrUpdatePlatform(conn, hx.alloc, .{
        .id = prepared.name,
        .name = prepared.name,
        .description = prepared.description,
        .source_repo = body.source_ref,
        .content_hash = prepared.content_hash,
        .skill_markdown = body.skill_markdown,
        .trigger_markdown = body.trigger_markdown,
        .requirements_json = prepared.requirements_json,
        .support_files_json = prepared.support_files_json,
        .now_ms = clock.nowMillis(),
    }) catch |err| {
        log.err("platform_onboard_store_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err), .req_id = hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return null;
    };
}

fn insertTenant(hx: Hx, workspace_id: []const u8, body: importer.ImportBody, prepared: importer.PreparedBundle) ?[]const u8 {
    const conn = hx.ctx.pool.acquire() catch {
        common.internalDbUnavailable(hx.res, hx.req_id);
        return null;
    };
    defer hx.ctx.pool.release(conn);
    const template_id = id_format.generateFleetTemplateId(hx.alloc) catch {
        common.internalOperationError(hx.res, "identifier generation failed", hx.req_id);
        return null;
    };
    defer hx.alloc.free(template_id);
    return template_store.insertOrFetchTenant(conn, hx.alloc, .{
        .id = template_id,
        .workspace_id = workspace_id,
        .name = prepared.name,
        .source_kind = body.source_kind,
        .source_ref = body.source_ref,
        .content_hash = prepared.content_hash,
        .skill_markdown = body.skill_markdown,
        .trigger_markdown = body.trigger_markdown,
        .support_files_json = prepared.support_files_json,
        .requirements_json = prepared.requirements_json,
        .now_ms = clock.nowMillis(),
    }) catch |err| {
        log.err("tenant_onboard_store_failed", .{ .error_code = ec.ERR_INTERNAL_DB_QUERY, .err = @errorName(err), .req_id = hx.req_id });
        common.internalDbError(hx.res, hx.req_id);
        return null;
    };
}

fn respond(hx: Hx, body: importer.ImportBody, prepared: importer.PreparedBundle, id: []const u8, tier: []const u8) void {
    const requirements = std.json.parseFromSlice(std.json.Value, hx.alloc, prepared.requirements_json, .{}) catch {
        common.internalOperationError(hx.res, "template requirements serialization failed", hx.req_id);
        return;
    };
    defer requirements.deinit();
    const summaries = pipeline.supportSummaries(hx.alloc, body.support_files) catch {
        common.internalOperationError(hx.res, "support summary allocation failed", hx.req_id);
        return;
    };
    defer hx.alloc.free(summaries);
    hx.ok(.created, .{
        .id = id,
        .name = prepared.name,
        .visibility = tier,
        .content_hash = prepared.content_hash,
        .requirements = requirements.value,
        .support_files = summaries,
    });
}
