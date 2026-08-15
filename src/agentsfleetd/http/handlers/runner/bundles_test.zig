//! Refusal proofs for the runner-plane Fleet Bundle download.
//!
//! The handler rebuilds an R2 key from a caller-supplied path segment, so the
//! order of its guards is the access boundary, not a detail: identity first,
//! then the hash shape, and only then anything that touches storage. The
//! existing test in `bundles.zig` covers `isContentHash` as a predicate; these
//! cover the guards as the handler actually runs them.
//!
//! The three arms past the storage check need a live R2 and are proven in the
//! integration suite; what is provable without one is that no request reaches
//! them unless it carries a runner identity and a well-formed digest.

const std = @import("std");
const httpz = @import("httpz");
const testing = std.testing;

const bundles = @import("bundles.zig");
const hx_mod = @import("../hx.zig");
const common = @import("../common.zig");
const ec = @import("../../../errors/error_registry.zig");

const Hx = hx_mod.Hx;

const REQ_ID = "req-bundle-1";
const RUNNER_ID = "01932b7c-0000-7000-8000-00000000000a";
const VALID_HASH = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const K_ERROR_CODE = "error_code";

fn buildHx(res: *httpz.Response, ctx: *common.Context, principal: common.AuthPrincipal) Hx {
    return Hx{
        .alloc = testing.allocator,
        .principal = principal,
        .req_id = REQ_ID,
        .ctx = ctx,
        .res = res,
    };
}

/// A runner that authenticated but resolved to no row — the shape a revoked or
/// half-provisioned token produces.
const ANONYMOUS_RUNNER = common.AuthPrincipal{ .mode = .runner };
const IDENTIFIED_RUNNER = common.AuthPrincipal{ .mode = .runner, .runner_id = RUNNER_ID };

/// Only `r2` is populated: every path below returns at or before the storage
/// check, so no other field is read.
fn buildCtxWithoutStorage() common.Context {
    // SAFETY: see above.
    var ctx: common.Context = undefined;
    ctx.r2 = null;
    return ctx;
}

test "should refuse a caller that carries no runner identity" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    var ctx = buildCtxWithoutStorage();

    bundles.innerRunnerBundle(buildHx(ht.res, &ctx, ANONYMOUS_RUNNER), VALID_HASH);

    const json = try ht.getJson();
    try testing.expectEqualStrings(ec.ERR_RUN_INVALID_RUNNER_TOKEN, json.object.get(K_ERROR_CODE).?.string);
}

test "should refuse every content ref that is not a lowercase sha256 digest" {
    const refs = [_][]const u8{
        "", // absent
        VALID_HASH[0..63], // one short
        VALID_HASH ++ "a", // one long
        "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855", // uppercase
        "../../etc/passwd000000000000000000000000000000000000000000000000", // traversal
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b8/5", // separator
    };
    for (refs) |ref| {
        var ht = httpz.testing.init(.{});
        defer ht.deinit();
        var ctx = buildCtxWithoutStorage();

        bundles.innerRunnerBundle(buildHx(ht.res, &ctx, IDENTIFIED_RUNNER), ref);

        const json = try ht.getJson();
        testing.expectEqualStrings(ec.ERR_INVALID_REQUEST, json.object.get(K_ERROR_CODE).?.string) catch |e| {
            std.debug.print("ref accepted or misrouted: \"{s}\"\n", .{ref});
            return e;
        };
    }
}

test "should reject a malformed ref before consulting storage at all" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    var ctx = buildCtxWithoutStorage();

    // Storage is unconfigured, so an answer of "storage unavailable" would mean
    // the handler reached the R2 branch with an unvalidated ref in hand. The
    // rebuilt key is only safe because this ordering holds.
    bundles.innerRunnerBundle(buildHx(ht.res, &ctx, IDENTIFIED_RUNNER), "../../../secrets");

    const json = try ht.getJson();
    const code = json.object.get(K_ERROR_CODE).?.string;
    try testing.expectEqualStrings(ec.ERR_INVALID_REQUEST, code);
    try testing.expect(!std.mem.eql(u8, ec.ERR_FLEET_BUNDLE_STORAGE_UNAVAILABLE, code));
}

test "should check the runner identity before the content ref" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    var ctx = buildCtxWithoutStorage();

    // Both guards would refuse this request. An unauthenticated caller must not
    // learn which refs are well-formed from the difference between the two.
    bundles.innerRunnerBundle(buildHx(ht.res, &ctx, ANONYMOUS_RUNNER), "not-a-digest");

    const json = try ht.getJson();
    try testing.expectEqualStrings(ec.ERR_RUN_INVALID_RUNNER_TOKEN, json.object.get(K_ERROR_CODE).?.string);
}

test "should report storage unavailable for a valid request when no store is configured" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();
    var ctx = buildCtxWithoutStorage();

    bundles.innerRunnerBundle(buildHx(ht.res, &ctx, IDENTIFIED_RUNNER), VALID_HASH);

    const json = try ht.getJson();
    try testing.expectEqualStrings(ec.ERR_FLEET_BUNDLE_STORAGE_UNAVAILABLE, json.object.get(K_ERROR_CODE).?.string);
}
