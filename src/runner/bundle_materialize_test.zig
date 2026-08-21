//! The bundle materialize arc: download via the daemon proxy, extract into the
//! workspace, write through the content-addressed cache.
//!
//! `extractSupportFiles` has its own suite; `materialize`/`cacheOrDownload`/
//! `downloadBundle` had no executed lines — the exact seam where a leased
//! bundle's bytes reach disk. Pinned against a scripted plane: the 200 path
//! extracts AND caches, the warm cache serves with the plane dead (the cache
//! must be sufficient, not decorative), a 404 is the skill-only no-op, and a
//! refusal is `.failed` with nothing written, carrying the cause the operator
//! reads: a plane refusal is `.download`, an unreadable archive `.malformed`.

const std = @import("std");
const testing = std.testing;
const common = @import("common");

const bundle_extract = @import("bundle_extract.zig");
const client_mod = @import("daemon/control_plane_client.zig");
const dts = @import("daemon/deadline_test_support.zig");
const plane_stub = @import("cmd/plane_stub_test.zig");

const ALLOC = testing.allocator;
const DEADLINE_MS: u31 = 2_000;
const HASH = "cafe0123deadbeef";
const SUPPORT_NAME = "playbook.md";
const SUPPORT_BODY = "# support file body\n";

fn freshDir(io: std.Io, comptime name: []const u8) ![]const u8 {
    const path = "/tmp/agentsfleet-bm-test-" ++ name;
    try std.Io.Dir.cwd().deleteTree(io, path); // idempotent on a missing path
    try std.Io.Dir.createDirAbsolute(io, path, .default_dir);
    return path;
}

fn buildTar(alloc: std.mem.Allocator) ![]u8 {
    // Mirrors bundle_extract_test's buildTar (the proven tar-writer idiom).
    var aw: std.Io.Writer.Allocating = .init(alloc);
    errdefer aw.deinit();
    var tw: std.tar.Writer = .{ .underlying_writer = &aw.writer };
    try tw.writeFileBytes(SUPPORT_NAME, SUPPORT_BODY, .{});
    try aw.writer.flush();
    return aw.toOwnedSlice();
}

/// Drive `materialize` once against a scripted plane response.
fn materializeAgainst(status: plane_stub.StubStatus, storage_home: []const u8, workspace: []const u8) !bundle_extract.MaterializeResult {
    const io = common.globalIo();
    var addr = try std.Io.net.IpAddress.parseIp4("127.0.0.1", 0);
    var listener = addr.listen(io, .{ .reuse_address = true }) catch return error.SkipZigTest;
    defer listener.deinit(io);
    const port = plane_stub.boundPort(listener.socket.handle) catch return error.SkipZigTest;
    var stub = plane_stub.OneShotPlane{ .io = io, .listener = &listener, .status = status };
    const responder = std.Thread.spawn(.{}, plane_stub.OneShotPlane.serve, .{&stub}) catch return error.SkipZigTest;
    defer responder.join();

    var url_buf: [48]u8 = undefined;
    const url = try std.fmt.bufPrint(&url_buf, "http://127.0.0.1:{d}", .{port});
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var c = client_mod.init(ALLOC, common.globalIo(), try deadlines.start(ALLOC), url);
    defer c.deinit();

    return bundle_extract.materialize(io, ALLOC, &c, "agt_rtest", storage_home, workspace, .{ .content_hash = HASH }, DEADLINE_MS);
}

test "a downloaded bundle extracts into the workspace and writes the cache" {
    const io = common.globalIo();
    const home = try freshDir(io, "dl-home");
    defer std.Io.Dir.cwd().deleteTree(io, home) catch |err| std.log.warn("cleanup: {s}", .{@errorName(err)});
    const ws = try freshDir(io, "dl-ws");
    defer std.Io.Dir.cwd().deleteTree(io, ws) catch |err| std.log.warn("cleanup: {s}", .{@errorName(err)});

    const tar = try buildTar(ALLOC);
    defer ALLOC.free(tar);

    const result = try materializeAgainst(.{ .line = "200 OK", .body = tar }, home, ws);
    try testing.expect(result == .ready);

    // The support file landed in the workspace…
    var path_buf: [256]u8 = undefined;
    const support_path = try std.fmt.bufPrint(&path_buf, "{s}/{s}", .{ ws, SUPPORT_NAME });
    const written = try std.Io.Dir.cwd().readFileAlloc(io, support_path, ALLOC, .limited(4096));
    defer ALLOC.free(written);
    try testing.expectEqualStrings(SUPPORT_BODY, written);

    // …and the canonical tar was written through to the content-addressed cache.
    const cached = bundle_extract.readCache(io, ALLOC, home, HASH) orelse return error.TestUnexpectedResult;
    defer ALLOC.free(cached);
    try testing.expectEqualSlices(u8, tar, cached);
}

test "a warm cache serves the bundle with the control plane unreachable" {
    const io = common.globalIo();
    const home = try freshDir(io, "cache-home");
    defer std.Io.Dir.cwd().deleteTree(io, home) catch |err| std.log.warn("cleanup: {s}", .{@errorName(err)});
    const ws = try freshDir(io, "cache-ws");
    defer std.Io.Dir.cwd().deleteTree(io, ws) catch |err| std.log.warn("cleanup: {s}", .{@errorName(err)});
    const ws2 = try freshDir(io, "cache-ws2");
    defer std.Io.Dir.cwd().deleteTree(io, ws2) catch |err| std.log.warn("cleanup: {s}", .{@errorName(err)});

    const tar = try buildTar(ALLOC);
    defer ALLOC.free(tar);
    // First materialize downloads and populates the cache.
    _ = try materializeAgainst(.{ .line = "200 OK", .body = tar }, home, ws);

    // Second lease, same hash, DEAD plane: the cache must carry it alone.
    var deadlines: dts.TestScheduler = .{};
    defer deadlines.deinit();
    var c = client_mod.init(ALLOC, common.globalIo(), try deadlines.start(ALLOC), "http://127.0.0.1:1");
    defer c.deinit();
    const result = bundle_extract.materialize(io, ALLOC, &c, "agt_rtest", home, ws2, .{ .content_hash = HASH }, DEADLINE_MS);
    try testing.expect(result == .ready);

    var path_buf: [256]u8 = undefined;
    const support_path = try std.fmt.bufPrint(&path_buf, "{s}/{s}", .{ ws2, SUPPORT_NAME });
    const written = try std.Io.Dir.cwd().readFileAlloc(io, support_path, ALLOC, .limited(4096));
    defer ALLOC.free(written);
    try testing.expectEqualStrings(SUPPORT_BODY, written);
}

test "a 404 is the skill-only bundle: ready, nothing extracted" {
    const io = common.globalIo();
    const home = try freshDir(io, "skill-home");
    defer std.Io.Dir.cwd().deleteTree(io, home) catch |err| std.log.warn("cleanup: {s}", .{@errorName(err)});
    const ws = try freshDir(io, "skill-ws");
    defer std.Io.Dir.cwd().deleteTree(io, ws) catch |err| std.log.warn("cleanup: {s}", .{@errorName(err)});

    const result = try materializeAgainst(.{ .line = "404 Not Found", .body = "" }, home, ws);
    try testing.expect(result == .ready);

    var path_buf: [256]u8 = undefined;
    const support_path = try std.fmt.bufPrint(&path_buf, "{s}/{s}", .{ ws, SUPPORT_NAME });
    try testing.expectError(error.FileNotFound, std.Io.Dir.cwd().readFileAlloc(io, support_path, ALLOC, .limited(4096)));
}

test "a refusal fails the materialize and writes nothing" {
    const io = common.globalIo();
    const home = try freshDir(io, "fail-home");
    defer std.Io.Dir.cwd().deleteTree(io, home) catch |err| std.log.warn("cleanup: {s}", .{@errorName(err)});
    const ws = try freshDir(io, "fail-ws");
    defer std.Io.Dir.cwd().deleteTree(io, ws) catch |err| std.log.warn("cleanup: {s}", .{@errorName(err)});

    const result = try materializeAgainst(.{ .line = "500 Internal Server Error", .body = "{}" }, home, ws);
    try testing.expectEqual(bundle_extract.MaterializeFailure.download, result.failed);
    try testing.expect(bundle_extract.readCache(io, ALLOC, home, HASH) == null);
}

test "a 200 carrying a body that is not a tar fails as malformed, not as a download failure" {
    // The two halves of materialize answer for different people: a refusal is the
    // plane's problem, an unreadable archive is the bundle's. One collapsed detail
    // told the operator neither.
    const io = common.globalIo();
    const home = try freshDir(io, "junk-home");
    defer std.Io.Dir.cwd().deleteTree(io, home) catch |err| std.log.warn("cleanup: {s}", .{@errorName(err)});
    const ws = try freshDir(io, "junk-ws");
    defer std.Io.Dir.cwd().deleteTree(io, ws) catch |err| std.log.warn("cleanup: {s}", .{@errorName(err)});

    const result = try materializeAgainst(.{ .line = "200 OK", .body = "not a tar archive at all" }, home, ws);
    try testing.expectEqual(bundle_extract.MaterializeFailure.malformed, result.failed);
}
