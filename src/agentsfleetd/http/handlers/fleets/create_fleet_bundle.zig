const std = @import("std");
const httpz = @import("httpz");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const store = @import("../../../fleet_library/store.zig");
const library_store = @import("../../../fleet_library/library_store.zig");
const importer = @import("../../../fleet_library/importer.zig");
const markdown_limits = @import("../../../fleet_runtime/markdown_limits.zig");

/// Install source — exactly one of the two onboarded Fleet library tiers
/// (M103 §4). A tagged union makes "exactly one" a compile-time guarantee —
/// the "both set" / "neither set" states cannot exist at the type level.
pub const SourceInput = union(enum) {
    platform: []const u8,
    tenant: []const u8,
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
    owned_entry: ?library_store.InstallEntry = null,
    owned_snapshot_key: ?[]const u8 = null,

    pub fn deinit(self: *const ResolvedSource, alloc: std.mem.Allocator) void {
        if (self.owned_entry) |t| t.deinit(alloc);
        if (self.owned_snapshot_key) |k| alloc.free(k);
    }
};

/// Resolve the install source to the SKILL/TRIGGER markdown + content identity of
/// an already-onboarded Fleet library entry. Returns `!?ResolvedSource`:
/// - `null` = entry not found (response already written via `hx.fail`)
/// - `error.Abort` = a post-acquire step failed (response written, entry cleaned
///   up by `errdefer` — the leak class this function previously had)
/// The caller does `catch return orelse return; defer source.deinit(alloc);`.
pub fn resolveSource(
    hx: hx_mod.Hx,
    conn: anytype,
    workspace_id: []const u8,
    input: SourceInput,
) !?ResolvedSource {
    if (input == .tenant and !common.requireUuidV7Id(hx.res, hx.req_id, input.tenant, "tenant_library_id")) return null;

    const fetched = switch (input) {
        .platform => |id| library_store.fetchPlatformInstall(conn, hx.alloc, id),
        .tenant => |id| library_store.fetchTenantInstall(conn, hx.alloc, workspace_id, id),
    } catch {
        common.internalDbError(hx.res, hx.req_id);
        return null;
    };
    const entry = fetched orelse {
        hx.fail(ec.ERR_FLEET_BUNDLE_NOT_FOUND, "library entry not found or not installable");
        return null;
    };
    errdefer entry.deinit(hx.alloc);

    const snapshot_key = importer.snapshotKey(hx.alloc, entry.content_hash) catch {
        common.internalOperationError(hx.res, "bundle key build failed", hx.req_id);
        return error.Abort;
    };
    errdefer hx.alloc.free(snapshot_key);

    return .{
        .source_markdown = entry.skill_markdown,
        .trigger_markdown = entry.trigger_markdown,
        .bundle_ref = .{ .content_hash = entry.content_hash, .snapshot_key = snapshot_key },
        .owned_entry = entry,
        .owned_snapshot_key = snapshot_key,
    };
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
    const missing = store.missingSecretNames(conn, hx.alloc, workspace_id, credentials) catch {
        common.internalDbError(hx.res, hx.req_id);
        return false;
    };
    defer store.freeStringSlice(hx.alloc, missing);
    if (missing.len == 0) return true;
    writeMissingSecrets(hx.res, hx.req_id, missing);
    return false;
}

fn writeMissingSecrets(res: *httpz.Response, request_id: []const u8, missing: []const []const u8) void {
    const entry = ec.lookup(ec.ERR_FLEET_BUNDLE_SECRETS_MISSING);
    res.status = @intFromEnum(entry.http_status);
    res.header(common.HEADER_CONTENT_TYPE, common.CONTENT_TYPE_PROBLEM_JSON);
    res.json(.{
        .docs_uri = entry.docs_uri,
        .title = entry.title,
        .detail = "Fleet Bundle requires workspace secrets that are not present",
        .error_code = ec.ERR_FLEET_BUNDLE_SECRETS_MISSING,
        .request_id = request_id,
        .missing_secrets = missing,
    }, .{}) catch {
        res.status = 500;
        res.body = "{}";
    };
}
