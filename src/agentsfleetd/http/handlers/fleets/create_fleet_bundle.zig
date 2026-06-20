const std = @import("std");
const httpz = @import("httpz");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const store = @import("../../../fleet_bundle/store.zig");
const markdown_limits = @import("../../../fleet_runtime/markdown_limits.zig");

pub const SourceInput = struct {
    source_markdown: ?[]const u8,
    trigger_markdown: ?[]const u8,
    bundle_id: ?[]const u8,
};

pub const BundleRef = struct {
    id: []const u8,
    content_hash: []const u8,
    snapshot_key: []const u8,
};

pub const ResolvedSource = struct {
    source_markdown: []const u8,
    trigger_markdown: ?[]const u8,
    bundle_ref: ?BundleRef = null,
    owned_bundle: ?store.InstallBundle = null,

    pub fn deinit(self: *const ResolvedSource, alloc: std.mem.Allocator) void {
        if (self.owned_bundle) |bundle| bundle.deinit(alloc);
    }
};

pub fn resolveSource(
    hx: hx_mod.Hx,
    conn: anytype,
    workspace_id: []const u8,
    input: SourceInput,
) ?ResolvedSource {
    if (input.bundle_id) |bundle_id| {
        if (!common.requireUuidV7Id(hx.res, hx.req_id, bundle_id, "bundle_id")) return null;
        const bundle = store.fetchForInstall(conn, hx.alloc, workspace_id, bundle_id) catch {
            common.internalDbError(hx.res, hx.req_id);
            return null;
        } orelse {
            hx.fail(ec.ERR_FLEET_BUNDLE_NOT_FOUND, "Fleet Bundle not found");
            return null;
        };
        return .{
            .source_markdown = input.source_markdown orelse bundle.skill_markdown,
            .trigger_markdown = input.trigger_markdown orelse bundle.trigger_markdown,
            .bundle_ref = .{
                .id = bundle.id,
                .content_hash = bundle.content_hash,
                .snapshot_key = bundle.snapshot_key,
            },
            .owned_bundle = bundle,
        };
    }

    const source = input.source_markdown orelse {
        hx.fail(ec.ERR_INVALID_REQUEST, ec.MSG_AGENTSFLEET_SOURCE_REQUIRED);
        return null;
    };
    return .{ .source_markdown = source, .trigger_markdown = input.trigger_markdown };
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
