//! Unit proofs for the shared bundle-onboarding pipeline helpers.
//!
//! `failImport` is the whole caller-visible surface of a rejected import: the
//! platform and tenant onboarding handlers hand it whatever `resolve`/`importer`
//! returned and write nothing themselves. So the arm-to-response map IS the API
//! error behaviour, and it is pinned here row by row — a refactor that re-points
//! one arm at a neighbouring code changes what an operator sees, silently,
//! because every arm still "works".
//!
//! These need only the response half of `Hx`; `ctx` and `principal` stay
//! undefined the way `hx_test.zig` establishes, except where `putSnapshot`
//! reads `ctx.r2`.

const std = @import("std");
const httpz = @import("httpz");
const testing = std.testing;

const pipeline = @import("pipeline.zig");
const hx_mod = @import("../hx.zig");
const common = @import("../common.zig");
const ec = @import("../../../errors/error_registry.zig");

const Hx = hx_mod.Hx;

const REQ_ID = "req-pipeline-1";
const K_ERROR_CODE = "error_code";
const K_DETAIL = "detail";

/// `failImport` reads only the response half — matching `hx_test.zig`'s note
/// that a future read of `ctx`/`principal` should crash loudly here rather than
/// pass against a fixture that quietly grew the coupling.
fn buildHx(res: *httpz.Response) Hx {
    return Hx{
        .alloc = testing.allocator,
        // SAFETY: failImport touches neither field; a change that makes it read
        // one crashes this test instead of hiding the new dependency.
        .principal = undefined,
        .req_id = REQ_ID,
        // SAFETY: as above.
        .ctx = undefined,
        .res = res,
    };
}

const Arm = struct { err: anyerror, code: []const u8, detail: []const u8 };

/// Every arm of the switch, with the pair a caller actually receives. The
/// details are distinct per arm on purpose: asserting the code alone would let
/// two arms swap their messages and stay green.
const ARMS = [_]Arm{
    .{ .err = error.MissingSkill, .code = ec.ERR_FLEET_BUNDLE_INVALID, .detail = "missing_skill" },
    .{ .err = error.UploadAttachmentsUnsupported, .code = ec.ERR_FLEET_BUNDLE_INVALID, .detail = "upload sources cannot carry support files; use a github or template source" },
    .{ .err = error.InvalidSourceRef, .code = ec.ERR_FLEET_BUNDLE_INVALID, .detail = "source_ref must be 'owner/repo' for a github source" },
    .{ .err = error.InvalidSource, .code = ec.ERR_FLEET_BUNDLE_INVALID, .detail = "source reference is not a valid GitHub owner/repo/ref" },
    .{ .err = error.InvalidSourceKind, .code = ec.ERR_FLEET_BUNDLE_INVALID, .detail = "source_kind must be template, upload, or github" },
    .{ .err = error.InvalidSkill, .code = ec.ERR_FLEET_BUNDLE_INVALID, .detail = "SKILL.md frontmatter is invalid" },
    .{ .err = error.InvalidTrigger, .code = ec.ERR_FLEET_BUNDLE_INVALID, .detail = "TRIGGER.md frontmatter is invalid" },
    .{ .err = error.NameMismatch, .code = ec.ERR_AGENTSFLEET_NAME_MISMATCH, .detail = ec.MSG_AGENTSFLEET_NAME_MISMATCH },
    .{ .err = error.UnsafePath, .code = ec.ERR_FLEET_BUNDLE_INVALID, .detail = "unsafe_path" },
    .{ .err = error.SecretShape, .code = ec.ERR_FLEET_BUNDLE_INVALID, .detail = "support files must not carry credential-shaped content" },
    .{ .err = error.TooLarge, .code = ec.ERR_PAYLOAD_TOO_LARGE, .detail = "Fleet Bundle exceeds a configured size cap" },
    .{ .err = error.TarballTooLarge, .code = ec.ERR_PAYLOAD_TOO_LARGE, .detail = "fetched Fleet Bundle exceeds the snapshot size cap" },
    .{ .err = error.TooManyFiles, .code = ec.ERR_PAYLOAD_TOO_LARGE, .detail = "fetched Fleet Bundle exceeds the file-count cap" },
    .{ .err = error.FetchFailed, .code = ec.ERR_FLEET_BUNDLE_FETCH_FAILED, .detail = "the Fleet Bundle source could not be fetched from GitHub" },
    .{ .err = error.InvalidUrl, .code = ec.ERR_FLEET_BUNDLE_FETCH_FAILED, .detail = "the Fleet Bundle source could not be fetched from GitHub" },
    .{ .err = error.DisallowedRedirect, .code = ec.ERR_FLEET_BUNDLE_FETCH_FAILED, .detail = "the GitHub source redirected to a disallowed host" },
    .{ .err = error.CorruptArchive, .code = ec.ERR_FLEET_BUNDLE_FETCH_FAILED, .detail = "the fetched Fleet Bundle archive could not be read" },
    .{ .err = error.OutOfMemory, .code = ec.ERR_INTERNAL_OPERATION_FAILED, .detail = "Failed to import the Fleet Bundle" },
};

test "should map every import failure to its own code and detail" {
    for (ARMS) |arm| {
        var ht = httpz.testing.init(.{});
        defer ht.deinit();

        pipeline.failImport(buildHx(ht.res), arm.err);

        const json = try ht.getJson();
        const got_code = json.object.get(K_ERROR_CODE).?.string;
        const got_detail = json.object.get(K_DETAIL).?.string;
        testing.expectEqualStrings(arm.code, got_code) catch |e| {
            std.debug.print("arm {s}: code mismatch\n", .{@errorName(arm.err)});
            return e;
        };
        testing.expectEqualStrings(arm.detail, got_detail) catch |e| {
            std.debug.print("arm {s}: detail mismatch\n", .{@errorName(arm.err)});
            return e;
        };
    }
}

test "should answer an unmapped import failure with the generic internal code" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    // Not an arm of the switch: an importer that grows a new error must degrade
    // to the generic internal answer rather than reaching a caller unhandled.
    pipeline.failImport(buildHx(ht.res), error.SomeUnclassifiedImporterFault);

    const json = try ht.getJson();
    try testing.expectEqualStrings(ec.ERR_INTERNAL_OPERATION_FAILED, json.object.get(K_ERROR_CODE).?.string);
    try testing.expectEqualStrings("bundle import failed", json.object.get(K_DETAIL).?.string);
}

test "should never answer an import failure with an unregistered error code" {
    // `hx.fail` substitutes the UNKNOWN envelope for a code the registry does
    // not carry, which turns a mistyped constant into a 500 that still looks
    // deliberate. Every arm must resolve to a registered code.
    for (ARMS) |arm| {
        var ht = httpz.testing.init(.{});
        defer ht.deinit();

        pipeline.failImport(buildHx(ht.res), arm.err);

        const json = try ht.getJson();
        const got_code = json.object.get(K_ERROR_CODE).?.string;
        try testing.expect(!std.mem.eql(u8, ec.UNKNOWN.code, got_code));
    }
}

test "should refuse the snapshot put when no object store is configured" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    // SAFETY: the R2-absent arm returns before reading any other Context field.
    var ctx: common.Context = undefined;
    ctx.r2 = null;
    var hx = buildHx(ht.res);
    hx.ctx = &ctx;

    // The bundle carries support files but there is nowhere to put the
    // canonical tar, so the R2-before-metadata invariant cannot be honoured and
    // the caller must not proceed to a metadata commit.
    const ok = pipeline.putSnapshot(hx, undefined, undefined);

    try testing.expect(!ok);
    const json = try ht.getJson();
    try testing.expectEqualStrings(ec.ERR_FLEET_BUNDLE_STORAGE_UNAVAILABLE, json.object.get(K_ERROR_CODE).?.string);
}
