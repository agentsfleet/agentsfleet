// Unit tests for the create path's two unstageable failure responses.
//
// Neither response can be reached by driving the handler: one needs the request
// arena to fail an allocation, the other needs an insert to fail for a reason
// other than the unique constraint, which this schema will not produce on
// demand. Both are emitted from `create_failure.zig` so they can be asserted
// against a real `httpz.Response` here — the split `hx_test.zig` already uses
// for `hx.ok` and `hx.fail`.
//
// What these prove:
//   - each helper writes its own error code at 500
//   - the RFC 7807 envelope carries the caller's request id
//   - the underlying error name reaches the log, never the response body

const std = @import("std");
const httpz = @import("httpz");
const hx_mod = @import("../hx.zig");
const create_failure = @import("create_failure.zig");
const error_codes = @import("../../../errors/error_registry.zig");

const Hx = hx_mod.Hx;

/// Both helpers read only `res` and `req_id`. If that ever changes these tests
/// crash rather than pass quietly, which surfaces the new coupling.
fn buildHx(res: *httpz.Response, req_id: []const u8) Hx {
    return Hx{
        .alloc = std.testing.allocator,
        // SAFETY: test fixture; neither helper under test reads this field.
        .principal = undefined,
        .req_id = req_id,
        // SAFETY: test fixture; neither helper under test reads this field.
        .ctx = undefined,
        .res = res,
    };
}

test "create failure: a name the server could not generate is a 500 UZ-INTERNAL-003" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    create_failure.nameGenerationFailed(buildHx(ht.res, "req-namegen-1"));

    try ht.expectStatus(500);
    const json = try ht.getJson();
    const obj = json.object;
    // Not the DB code: nothing is wrong with the datastore, and an on-call
    // paged for a database fault by this path would be chasing the wrong thing.
    try std.testing.expectEqualStrings(error_codes.ERR_INTERNAL_OPERATION_FAILED, obj.get("error_code").?.string);
    try std.testing.expectEqualStrings("name generation failed", obj.get("detail").?.string);
    try std.testing.expectEqualStrings("req-namegen-1", obj.get("request_id").?.string);
}

test "create failure: a non-unique insert failure is a 500 UZ-INTERNAL-002" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    create_failure.insertFailed(buildHx(ht.res, "req-insert-1"), error.ConnectionResetByPeer);

    try ht.expectStatus(500);
    const json = try ht.getJson();
    const obj = json.object;
    try std.testing.expectEqualStrings(error_codes.ERR_INTERNAL_DB_QUERY, obj.get("error_code").?.string);
    try std.testing.expectEqualStrings("req-insert-1", obj.get("request_id").?.string);
}

test "create failure: the insert response is application/problem+json" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    create_failure.insertFailed(buildHx(ht.res, "req-insert-2"), error.PG);

    try ht.expectHeader("Content-Type", "application/problem+json");
}

test "create failure: the underlying error name never reaches the response body" {
    // The log line is the only place the cause survives. A future refactor that
    // pipes `@errorName(err)` into the detail would hand a caller the shape of
    // an internal failure, so pin the absence rather than trusting review.
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    create_failure.insertFailed(buildHx(ht.res, "req-insert-3"), error.ConnectionResetByPeer);

    try std.testing.expect(std.mem.indexOf(u8, ht.res.body, "ConnectionResetByPeer") == null);
    try std.testing.expectEqualStrings("Database error", (try ht.getJson()).object.get("detail").?.string);
}
