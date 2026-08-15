//! Per-lease workspace lifecycle from `lease_run.zig`.
//!
//! `prepareWorkspace`/`cleanupWorkspace` bracket every execution and had no
//! executed lines. Their contracts are small but load-bearing: an
//! already-existing directory is adoption, not failure (a crashed run's
//! residue must not brick the lease), and cleanup of a vanished workspace is
//! silent (double-cleanup races with the boot sweeper).

const std = @import("std");
const common = @import("common");

const lease_run = @import("lease_run.zig");

fn tmpBase(buf: []u8) ![]const u8 {
    // Deep, absolute, per-process — no collision across parallel lanes.
    return std.fmt.bufPrint(buf, "/tmp/agentsfleet-lease-ws-{d}", .{std.c.getpid()});
}

test "prepareWorkspace creates the per-lease directory and returns its path" {
    const io = common.globalIo();
    var base_buf: [64]u8 = undefined;
    const base = try tmpBase(&base_buf);
    std.Io.Dir.createDirAbsolute(io, base, .default_dir) catch {};
    defer std.Io.Dir.cwd().deleteTree(io, base) catch {};

    var ws_buf: [std.fs.max_path_bytes]u8 = undefined;
    const path = lease_run.prepareWorkspace(io, &ws_buf, base, "lease-ws-a") orelse
        return error.TestUnexpectedResult;

    try std.testing.expect(std.mem.endsWith(u8, path, "/lease-ws-a"));
    var dir = try std.Io.Dir.openDirAbsolute(io, path, .{});
    dir.close(io);
}

test "prepareWorkspace adopts an already-existing directory instead of failing" {
    // A crashed prior run leaves the directory behind; the retry must reuse it.
    const io = common.globalIo();
    var base_buf: [64]u8 = undefined;
    const base = try tmpBase(&base_buf);
    std.Io.Dir.createDirAbsolute(io, base, .default_dir) catch {};
    defer std.Io.Dir.cwd().deleteTree(io, base) catch {};

    var ws_buf: [std.fs.max_path_bytes]u8 = undefined;
    const first = lease_run.prepareWorkspace(io, &ws_buf, base, "lease-ws-b") orelse
        return error.TestUnexpectedResult;
    _ = first;
    var second_buf: [std.fs.max_path_bytes]u8 = undefined;
    const second = lease_run.prepareWorkspace(io, &second_buf, base, "lease-ws-b") orelse
        return error.TestUnexpectedResult;
    try std.testing.expect(std.mem.endsWith(u8, second, "/lease-ws-b"));
}

test "prepareWorkspace reports failure when the base cannot hold a directory" {
    // An unwritable/absent base is the hot-spin case the caller backs off on —
    // it must surface as null, not a panic and not a fabricated path.
    const io = common.globalIo();
    var ws_buf: [std.fs.max_path_bytes]u8 = undefined;
    const missing_base = "/tmp/agentsfleet-lease-ws-absent/definitely/missing";
    try std.testing.expect(lease_run.prepareWorkspace(io, &ws_buf, missing_base, "lease-ws-c") == null);
}

test "cleanupWorkspace removes the tree and is silent when it is already gone" {
    const io = common.globalIo();
    var base_buf: [64]u8 = undefined;
    const base = try tmpBase(&base_buf);
    std.Io.Dir.createDirAbsolute(io, base, .default_dir) catch {};
    defer std.Io.Dir.cwd().deleteTree(io, base) catch {};

    var ws_buf: [std.fs.max_path_bytes]u8 = undefined;
    const path = lease_run.prepareWorkspace(io, &ws_buf, base, "lease-ws-d") orelse
        return error.TestUnexpectedResult;

    lease_run.cleanupWorkspace(io, path);
    try std.testing.expectError(error.FileNotFound, std.Io.Dir.openDirAbsolute(io, path, .{}));

    // Second cleanup of the same path: the vanished tree is logged-and-ignored.
    lease_run.cleanupWorkspace(io, path);
}
