//! Shared bundle-onboarding pipeline helpers (M103). The platform and tenant
//! onboarding handlers run the same resolve → prepare → R2 flow; these are the
//! pieces both reuse: the R2-before-metadata snapshot put, the support-file
//! summary projection, and the resolve/import error → HTTP mapping. Extracted
//! here when the legacy per-workspace bundle import endpoint was removed.

const std = @import("std");

const common = @import("../common.zig");
const hx_mod = @import("../hx.zig");
const ec = @import("../../../errors/error_registry.zig");
const importer = @import("../../../fleet_bundle/importer.zig");
const resolve = @import("../fleet_bundles/resolve.zig");

const Hx = hx_mod.Hx;

pub const SupportFileSummary = struct {
    path: []const u8,
    size_bytes: usize,
};

/// Build {path, size_bytes} summaries from the live body's support files.
pub fn supportSummaries(alloc: std.mem.Allocator, files: []const importer.SupportFile) ![]SupportFileSummary {
    const summaries = try alloc.alloc(SupportFileSummary, files.len);
    for (files, 0..) |file, i| {
        summaries[i] = .{ .path = file.path, .size_bytes = file.content.len };
    }
    return summaries;
}

/// Write the canonical tar to R2 under the content-addressed snapshot key, before
/// any metadata commit (R2-before-metadata invariant). Returns false (and
/// responds) when R2 is unset or the put fails. Only called when the bundle
/// carries support files.
pub fn putSnapshot(hx: Hx, resolved: *const resolve.Resolved, prepared: importer.PreparedBundle) bool {
    const r2 = hx.ctx.r2 orelse {
        storageUnavailable(hx);
        return false;
    };
    const fb = if (resolved.fetched) |*f| f else {
        common.internalOperationError(hx.res, "Failed to process the Fleet Bundle's support files", hx.req_id);
        return false;
    };
    const canonical = fb.canonicalTar(hx.alloc) catch {
        common.internalOperationError(hx.res, "Failed to process the Fleet Bundle", hx.req_id);
        return false;
    };
    defer hx.alloc.free(canonical);
    r2.put(prepared.snapshot_key, canonical) catch {
        storageUnavailable(hx);
        return false;
    };
    return true;
}

fn storageUnavailable(hx: Hx) void {
    common.errorResponse(hx.res, ec.ERR_FLEET_BUNDLE_STORAGE_UNAVAILABLE, "Fleet Bundle snapshot storage is unavailable", hx.req_id);
}

/// Map a resolve/import error to its HTTP response.
pub fn failImport(hx: Hx, err: anyerror) void {
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
        error.OutOfMemory => common.internalOperationError(hx.res, "Failed to import the Fleet Bundle", hx.req_id),
        else => common.internalOperationError(hx.res, "bundle import failed", hx.req_id),
    }
}
