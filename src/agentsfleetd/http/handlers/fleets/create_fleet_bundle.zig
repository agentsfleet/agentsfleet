const std = @import("std");
const httpz = @import("httpz");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const store = @import("../../../fleet_bundle/store.zig");
const template_store = @import("../../../fleet_bundle/template_store.zig");
const importer = @import("../../../fleet_bundle/importer.zig");
const markdown_limits = @import("../../../fleet_runtime/markdown_limits.zig");

/// Install source — exactly one of the two onboarded template tiers (M103 §4).
/// Raw-SKILL paste and the legacy per-workspace bundle_id are no longer accepted;
/// a GitHub source is installed by first onboarding it as a tenant template.
pub const SourceInput = struct {
    platform_template_id: ?[]const u8 = null,
    tenant_template_id: ?[]const u8 = null,
};

/// The content identity a fleet row records for runner materialization. The
/// canonical tar lives in R2 under `snapshot_key`; the runner downloads it by
/// `content_hash`.
pub const BundleRef = struct {
    content_hash: []const u8,
    snapshot_key: []const u8,
};

pub const ResolvedSource = struct {
    source_markdown: []const u8,
    trigger_markdown: ?[]const u8,
    bundle_ref: ?BundleRef = null,
    owned_template: ?template_store.InstallTemplate = null,
    owned_snapshot_key: ?[]const u8 = null,

    pub fn deinit(self: *const ResolvedSource, alloc: std.mem.Allocator) void {
        if (self.owned_template) |t| t.deinit(alloc);
        if (self.owned_snapshot_key) |k| alloc.free(k);
    }
};

const Tier = enum { platform, tenant };

/// Resolve the install source to the SKILL/TRIGGER markdown + content identity of
/// an already-onboarded template. Rejects a payload that names zero or both
/// sources. A tenant template is visible only to its owning workspace.
pub fn resolveSource(
    hx: hx_mod.Hx,
    conn: anytype,
    workspace_id: []const u8,
    input: SourceInput,
) ?ResolvedSource {
    const tier = selectTier(hx, input) orelse return null;
    if (tier == .tenant and !common.requireUuidV7Id(hx.res, hx.req_id, input.tenant_template_id.?, "tenant_template_id")) return null;

    const fetched = switch (tier) {
        .platform => template_store.fetchPlatformInstall(conn, hx.alloc, input.platform_template_id.?),
        .tenant => template_store.fetchTenantInstall(conn, hx.alloc, workspace_id, input.tenant_template_id.?),
    } catch {
        common.internalDbError(hx.res, hx.req_id);
        return null;
    };
    const template = fetched orelse {
        hx.fail(ec.ERR_FLEET_BUNDLE_NOT_FOUND, "template not found or not installable");
        return null;
    };
    errdefer template.deinit(hx.alloc);

    const snapshot_key = importer.snapshotKey(hx.alloc, template.content_hash) catch {
        common.internalOperationError(hx.res, "bundle key build failed", hx.req_id);
        return null;
    };
    return .{
        .source_markdown = template.skill_markdown,
        .trigger_markdown = template.trigger_markdown,
        .bundle_ref = .{ .content_hash = template.content_hash, .snapshot_key = snapshot_key },
        .owned_template = template,
        .owned_snapshot_key = snapshot_key,
    };
}

fn selectTier(hx: hx_mod.Hx, input: SourceInput) ?Tier {
    const has_platform = input.platform_template_id != null;
    const has_tenant = input.tenant_template_id != null;
    if (has_platform and has_tenant) {
        hx.fail(ec.ERR_INVALID_REQUEST, "install accepts exactly one of platform_template_id or tenant_template_id");
        return null;
    }
    if (has_platform) return .platform;
    if (has_tenant) return .tenant;
    hx.fail(ec.ERR_INVALID_REQUEST, "install requires platform_template_id or tenant_template_id");
    return null;
}

pub fn validateFields(hx: hx_mod.Hx, source_markdown: []const u8, trigger_markdown: ?[]const u8) bool {
    if (source_markdown.len == 0 or source_markdown.len > markdown_limits.MAX_SOURCE_LEN) {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_AGENTSFLEET_SOURCE_REQUIRED);
        return false;
    }
    if (trigger_markdown) |tm| {
        if (tm.len == 0 or tm.len > markdown_limits.MAX_TRIGGER_LEN) {
            hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_AGENTSFLEET_TRIGGER_REQUIRED);
            return false;
        }
    }
    return true;
}

pub fn ensureBundleCredentials(
    hx: hx_mod.Hx,
    conn: anytype,
    workspace_id: []const u8,
    bundle_ref: ?BundleRef,
    credentials: []const []const u8,
) bool {
    if (bundle_ref == null or credentials.len == 0) return true;
    const missing = store.missingCredentialNames(conn, hx.alloc, workspace_id, credentials) catch {
        common.internalDbError(hx.res, hx.req_id);
        return false;
    };
    defer store.freeStringSlice(hx.alloc, missing);
    if (missing.len == 0) return true;
    writeMissingCredentials(hx.res, hx.req_id, missing);
    return false;
}

fn writeMissingCredentials(res: *httpz.Response, request_id: []const u8, missing: []const []const u8) void {
    const entry = ec.lookup(ec.ERR_FLEET_BUNDLE_CREDENTIALS_MISSING);
    res.status = @intFromEnum(entry.http_status);
    res.header(common.HEADER_CONTENT_TYPE, common.CONTENT_TYPE_PROBLEM_JSON);
    res.json(.{
        .docs_uri = entry.docs_uri,
        .title = entry.title,
        .detail = "Fleet Bundle requires workspace credentials that are not present",
        .error_code = ec.ERR_FLEET_BUNDLE_CREDENTIALS_MISSING,
        .request_id = request_id,
        .missing_credentials = missing,
    }, .{}) catch {
        res.status = 500;
        res.body = "{}";
    };
}
