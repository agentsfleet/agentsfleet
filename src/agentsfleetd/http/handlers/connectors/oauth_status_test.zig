//! Tests for the generic OAuth status hook: connected/not_connected mapping
//! and the guarantee that only the display label — never a secret field —
//! leaves the vault handle.

const std = @import("std");
const httpz = @import("httpz");
const hx_mod = @import("../hx.zig");
const oauth_status = @import("oauth_status.zig");

const Hx = hx_mod.Hx;

fn buildHx(res: *httpz.Response, req_id: []const u8) Hx {
    return Hx{
        .alloc = std.testing.allocator,
        // respondStatus only writes through ok/fail, which never read these —
        // a future read crashes here loudly and surfaces the coupling.
        // SAFETY: test fixture; populated by the surrounding builder before any read.
        .principal = undefined,
        .req_id = req_id,
        // SAFETY: test fixture; populated by the surrounding builder before any read.
        .ctx = undefined,
        .res = res,
    };
}

test "a null vault handle reads not_connected with a null label" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    oauth_status.respondStatus(buildHx(ht.res, "req-oauth-null"), null);

    try ht.expectStatus(200);
    const json = try ht.getJson();
    try std.testing.expectEqualStrings("not_connected", json.object.get("status").?.string);
    try std.testing.expect(json.object.get("label").? == .null);
}

test "a handle without the integration marker is not_connected, whatever else it carries" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    // A stored secret whose shape predates the marker — or an unrelated vault
    // row — must read as disconnected rather than leak a connected status.
    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(std.testing.allocator);
    try obj.put(std.testing.allocator, "access_token", .{ .string = "xoxb-not-yours" });

    oauth_status.respondStatus(buildHx(ht.res, "req-oauth-unmarked"), obj);

    try ht.expectStatus(200);
    const json = try ht.getJson();
    try std.testing.expectEqualStrings("not_connected", json.object.get("status").?.string);
    // The secret never crosses into the response body.
    try std.testing.expect(std.mem.indexOf(u8, try ht.getBody(), "xoxb-not-yours") == null);
}

test "a marked handle is connected and surfaces its string label only" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(std.testing.allocator);
    try obj.put(std.testing.allocator, "integration", .{ .string = "linear" });
    try obj.put(std.testing.allocator, "label", .{ .string = "Linear (megam)" });
    try obj.put(std.testing.allocator, "refresh_token", .{ .string = "rt-secret" });

    oauth_status.respondStatus(buildHx(ht.res, "req-oauth-ok"), obj);

    try ht.expectStatus(200);
    const json = try ht.getJson();
    try std.testing.expectEqualStrings("connected", json.object.get("status").?.string);
    try std.testing.expectEqualStrings("Linear (megam)", json.object.get("label").?.string);
    try std.testing.expect(std.mem.indexOf(u8, try ht.getBody(), "rt-secret") == null);
}

test "a non-string label degrades to null rather than an error" {
    var ht = httpz.testing.init(.{});
    defer ht.deinit();

    var obj: std.json.ObjectMap = .empty;
    defer obj.deinit(std.testing.allocator);
    try obj.put(std.testing.allocator, "integration", .{ .string = "linear" });
    try obj.put(std.testing.allocator, "label", .{ .integer = 7 });

    oauth_status.respondStatus(buildHx(ht.res, "req-oauth-badlabel"), obj);

    try ht.expectStatus(200);
    const json = try ht.getJson();
    try std.testing.expectEqualStrings("connected", json.object.get("status").?.string);
    try std.testing.expect(json.object.get("label").? == .null);
}
