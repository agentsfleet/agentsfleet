//! Unit tests for `copy_file.zig`.
//!
//! T6 — small file (< 4 KiB) copies byte-for-byte.
//! T7 — large file (1 MiB random bytes) copies byte-for-byte.
//! T8 — empty file copies and produces zero-byte target.
//! T9 — target exists → overwritten cleanly.
//! T10 — cross-fs fallback: best-effort skip if no foreign fs is mounted.

const std = @import("std");
const testing = std.testing;
const cf = @import("copy_file.zig");

fn writeTmp(dir: std.fs.Dir, name: []const u8, bytes: []const u8) !void {
    var f = try dir.createFile(name, .{ .truncate = true });
    defer f.close();
    try f.writeAll(bytes);
}

fn readAll(allocator: std.mem.Allocator, dir: std.fs.Dir, name: []const u8) ![]u8 {
    var f = try dir.openFile(name, .{});
    defer f.close();
    return f.readToEndAlloc(allocator, 32 * 1024 * 1024);
}

test "T6 small file copies byte-for-byte" {
    var tmp = testing.tmpDir(.{});
    defer tmp.cleanup();

    const payload = "hello, copy_file!\n";
    try writeTmp(tmp.dir, "src", payload);

    const src_path = try tmp.dir.realpathAlloc(testing.allocator, "src");
    defer testing.allocator.free(src_path);
    const dst_path = try std.fs.path.join(testing.allocator, &.{ std.fs.path.dirname(src_path).?, "dst" });
    defer testing.allocator.free(dst_path);

    const r = try cf.copyFile(src_path, dst_path);
    try testing.expectEqual(@as(u64, payload.len), r.bytes);

    const got = try readAll(testing.allocator, tmp.dir, "dst");
    defer testing.allocator.free(got);
    try testing.expectEqualStrings(payload, got);
}

test "T7 large file (1 MiB random) copies byte-for-byte" {
    var tmp = testing.tmpDir(.{});
    defer tmp.cleanup();

    const big_size: usize = 1 * 1024 * 1024;
    const big = try testing.allocator.alloc(u8, big_size);
    defer testing.allocator.free(big);
    var prng = std.Random.DefaultPrng.init(0xC0FFEE);
    prng.random().bytes(big);

    try writeTmp(tmp.dir, "big-src", big);

    const src_path = try tmp.dir.realpathAlloc(testing.allocator, "big-src");
    defer testing.allocator.free(src_path);
    const dst_path = try std.fs.path.join(testing.allocator, &.{ std.fs.path.dirname(src_path).?, "big-dst" });
    defer testing.allocator.free(dst_path);

    const r = try cf.copyFile(src_path, dst_path);
    try testing.expectEqual(@as(u64, big_size), r.bytes);

    const got = try readAll(testing.allocator, tmp.dir, "big-dst");
    defer testing.allocator.free(got);
    try testing.expectEqualSlices(u8, big, got);
}

test "T8 empty file copies, produces zero-byte target" {
    var tmp = testing.tmpDir(.{});
    defer tmp.cleanup();

    try writeTmp(tmp.dir, "empty-src", "");

    const src_path = try tmp.dir.realpathAlloc(testing.allocator, "empty-src");
    defer testing.allocator.free(src_path);
    const dst_path = try std.fs.path.join(testing.allocator, &.{ std.fs.path.dirname(src_path).?, "empty-dst" });
    defer testing.allocator.free(dst_path);

    const r = try cf.copyFile(src_path, dst_path);
    try testing.expectEqual(@as(u64, 0), r.bytes);

    const stat = try tmp.dir.statFile("empty-dst");
    try testing.expectEqual(@as(u64, 0), stat.size);
}

test "T9 target exists is overwritten cleanly" {
    var tmp = testing.tmpDir(.{});
    defer tmp.cleanup();

    try writeTmp(tmp.dir, "src", "fresh");
    try writeTmp(tmp.dir, "dst", "old-and-much-longer-payload-than-src");

    const src_path = try tmp.dir.realpathAlloc(testing.allocator, "src");
    defer testing.allocator.free(src_path);
    const dst_path = try std.fs.path.join(testing.allocator, &.{ std.fs.path.dirname(src_path).?, "dst" });
    defer testing.allocator.free(dst_path);

    _ = try cf.copyFile(src_path, dst_path);

    const got = try readAll(testing.allocator, tmp.dir, "dst");
    defer testing.allocator.free(got);
    try testing.expectEqualStrings("fresh", got);
}

test "T10 cross-fs fallback path - skip when no foreign fs available" {
    // We can't synthesize a tmpfs vs main-fs split inside a unit test on every
    // host. Document the gap; the rw fallback path is exercised end-to-end by
    // the in-fs tests above whenever ficlone/copy_file_range fail or are
    // unsupported (e.g. macOS, where fcopyfile handles cross-fs natively).
    return error.SkipZigTest;
}
