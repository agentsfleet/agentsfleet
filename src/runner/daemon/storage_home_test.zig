//! Filesystem tests for the storage-home claim and its startup orphan sweep
//! (`StorageHome.zig`). Each test owns a real temp directory under `/tmp` — the
//! sweep's whole job is to delete directories, so an in-memory fake would prove
//! nothing about the thing that can go wrong.
//!
//! `/tmp` is a symlink to `/private/tmp` on macOS, so every claim here also
//! exercises the canonicalization step rather than asserting it separately.
//!
//! The classifiers (`isLeaseWorkspaceName`, `isSweepablePath`) are pure and are
//! pinned inline in `StorageHome.zig`, next to the prose they encode.

const std = @import("std");
const StorageHome = @import("StorageHome.zig");

const io = @import("common").globalIo();
const Dir = std.Io.Dir;

/// Two well-formed lease ids — the shape `fleet/service.zig` mints and
/// `lease_run.prepareWorkspace` names a workspace after.
const LEASE_A = "0199a4c1-8f3e-7b21-9c4d-2f6a1e8b7d05";
const LEASE_B = "0199a4c2-1b77-7f40-8e13-5a9c0d2e4f61";
/// The bundle cache's dot-prefixed name — kept by the per-lease cleanup today,
/// and kept by the sweep for the same reason.
const CACHE_DIR = ".bundle-cache";

/// Fresh absolute temp home for one test; a stale tree from a previous run is
/// deleted first and the tree is deleted again on exit. Depth 2 under the root,
/// so it clears the shallow-path floor exactly as the shipped default does.
fn freshHome(comptime name: []const u8) ![]const u8 {
    const path = "/tmp/agentsfleet-sh-test-" ++ name;
    try Dir.cwd().deleteTree(io, path); // idempotent on a missing path
    try Dir.createDirAbsolute(io, path, .default_dir);
    return path;
}

fn makeDir(home: []const u8, name: []const u8) !void {
    var dir = try Dir.openDirAbsolute(io, home, .{});
    defer dir.close(io);
    try dir.createDir(io, name, .default_dir);
}

fn exists(home: []const u8, name: []const u8) bool {
    var dir = Dir.openDirAbsolute(io, home, .{}) catch return false;
    defer dir.close(io);
    _ = dir.statFile(io, name, .{ .follow_symlinks = false }) catch return false;
    return true;
}

/// Run one full boot against `home`, closing the claim afterwards. Returns what
/// the sweep did — the daemon holds its claim for the process lifetime, but a
/// test wants the home released so the next boot in the same test can claim it.
fn boot(home: []const u8) StorageHome.Outcome {
    var startup = StorageHome.claimAndSweep(io, home);
    if (startup.home) |*h| h.close(io);
    return startup.outcome;
}

/// Assert the refusal (or adoption) an outcome names, without reaching into a
/// payload that variant does not carry.
fn expectOutcome(expected: std.meta.Tag(StorageHome.Outcome), actual: StorageHome.Outcome) !void {
    try std.testing.expectEqual(expected, std.meta.activeTag(actual));
}

/// Assert the outcome reaped exactly `expected` workspaces. A non-`reaped`
/// outcome fails by name rather than by tripping the payload's safety check.
fn expectReaped(expected: u32, actual: StorageHome.Outcome) !void {
    switch (actual) {
        .reaped => |count| try std.testing.expectEqual(expected, count),
        else => {
            std.debug.print("expected reaped={d}, got .{s}\n", .{ expected, @tagName(actual) });
            return error.TestUnexpectedResult;
        },
    }
}

test "test_startup_sweep_reaps_orphans" {
    // Dimension 4.12. An unclean shutdown (SIGKILL / OOM kill / reboot) skips
    // `defer cleanupWorkspace`, so a per-lease workspace outlives its lease with
    // no collector. At the next boot no lease is held, so it is orphaned by
    // definition — and the dot-prefixed bundle cache, which every run shares,
    // is not.
    const home = try freshHome("reaps-orphans");
    defer Dir.cwd().deleteTree(io, home) catch {};
    try makeDir(home, LEASE_A);
    try makeDir(home, CACHE_DIR);

    // First boot ADOPTS: this daemon had never owned the directory, so it marks
    // it and reaps nothing. A fresh home has no orphans, so the only cost is one
    // restart on a home that predates the sweep.
    try expectOutcome(.adopted, boot(home));
    try std.testing.expect(exists(home, LEASE_A));

    // Second boot: the sentinel is ours, so the orphan goes and the cache stays.
    try expectReaped(1, boot(home));
    try std.testing.expect(!exists(home, LEASE_A));
    try std.testing.expect(exists(home, CACHE_DIR));
}

test "a second daemon on the same home does not reap the first daemon's live work" {
    // The rolling-deploy case: the outgoing daemon is still executing leases
    // while the incoming one boots. Its workspaces are live, not orphaned — and
    // the incoming daemon cannot tell the difference, so the lock decides.
    const home = try freshHome("contended");
    defer Dir.cwd().deleteTree(io, home) catch {};
    try expectOutcome(.adopted, boot(home)); // sentinel now present

    // Real boot order: the incumbent claims and sweeps an empty home, and only
    // then starts leasing work — so its workspace appears AFTER its own sweep.
    var incumbent = StorageHome.claimAndSweep(io, home);
    defer if (incumbent.home) |*h| h.close(io);
    try expectReaped(0, incumbent.outcome);
    try makeDir(home, LEASE_A); // now executing a lease

    // The incumbent holds the lock for its whole life, so the challenger is
    // refused and — the point — reaps nothing.
    try expectOutcome(.contended, boot(home));
    try std.testing.expect(exists(home, LEASE_A));
}

test "a path too shallow to be a storage home is refused before anything is written" {
    // A stray `RUNNER_STORAGE_HOME` (a truncated template value, a bad default)
    // must not turn the sweep loose on host data. The filesystem root is the
    // case that is shallow on every platform — `/tmp` is NOT, because macOS
    // canonicalizes it to `/private/tmp` and it clears the depth floor there.
    // The floor is the coarse filter; the sentinel and the lease-id name shape
    // are what actually bound a plausible-looking home. `isSweepablePath` pins
    // the rule itself, platform-free, inline in `StorageHome.zig`.
    var startup = StorageHome.claimAndSweep(io, "/");
    defer if (startup.home) |*h| h.close(io);
    try expectOutcome(.refused_shallow, startup.outcome);
    try std.testing.expect(startup.home == null); // nothing to close

    // The refusal precedes the lock and the sentinel, so a refused home is never
    // marked — nothing was written to the root.
    try std.testing.expect(!exists("/", ".agentsfleet-runner-home"));
    try std.testing.expect(!exists("/", ".agentsfleet-runner-home.lock"));
}

test "only lease-shaped directories are reaped: names, symlinks, and files survive" {
    const home = try freshHome("selective");
    defer Dir.cwd().deleteTree(io, home) catch {};
    const outside = try freshHome("selective-target"); // a tree the sweep must not reach
    defer Dir.cwd().deleteTree(io, outside) catch {};

    try makeDir(home, LEASE_A);
    try makeDir(home, "not-a-lease-id");
    try makeDir(home, CACHE_DIR);
    {
        var dir = try Dir.openDirAbsolute(io, home, .{});
        defer dir.close(io);
        // A lease-shaped SYMLINK pointing out of the home: the iterator reports
        // `.sym_link`, never `.directory`, so it is skipped rather than followed.
        try dir.symLink(io, outside, LEASE_B, .{});
        // A lease-shaped regular FILE is not a workspace either.
        (try dir.createFile(io, "0199a4c3-2c88-7a01-b555-6d1f3e0a9c72", .{})).close(io);
    }

    try expectOutcome(.adopted, boot(home));
    try expectReaped(1, boot(home)); // LEASE_A alone

    try std.testing.expect(!exists(home, LEASE_A));
    try std.testing.expect(exists(home, "not-a-lease-id"));
    try std.testing.expect(exists(home, CACHE_DIR));
    try std.testing.expect(exists(home, LEASE_B)); // the link itself
    try std.testing.expect(exists(home, "0199a4c3-2c88-7a01-b555-6d1f3e0a9c72"));
    // And the link's target is still openable — the sweep never crossed it.
    var target = try Dir.openDirAbsolute(io, outside, .{});
    target.close(io);
}

test "a sweep clears more orphans than one batch holds" {
    // The pass loop exists because deleting entries mid-iteration lets the
    // readdir cursor skip the ones it has not yet returned. Seed past one batch
    // so the multi-pass path runs and every orphan still goes.
    const home = try freshHome("many");
    defer Dir.cwd().deleteTree(io, home) catch {};
    const count = 70; // > SWEEP_BATCH (64)
    var name: [36]u8 = LEASE_A.*;
    for (0..count) |i| {
        // Vary the last two hex digits: still canonical, still 36 chars.
        name[34] = std.fmt.digitToChar(@intCast(i / 16), .lower);
        name[35] = std.fmt.digitToChar(@intCast(i % 16), .lower);
        try makeDir(home, &name);
    }
    try makeDir(home, CACHE_DIR);

    try expectOutcome(.adopted, boot(home));
    try expectReaped(count, boot(home));
    try std.testing.expect(exists(home, CACHE_DIR));
    try expectReaped(0, boot(home)); // idempotent
}

/// The lock file's name, mirrored from `StorageHome.zig`'s private constant so
/// a test can pre-create it. Kept private there because nothing but the claim
/// has any business naming it.
const LOCK_NAME = ".agentsfleet-runner-home.lock";

/// chmod by absolute path. `setPermissions` is only permitted on a handle that
/// was opened for iteration, so the helper opens it that way.
fn setMode(path: []const u8, mode: std.posix.mode_t) !void {
    var dir = try Dir.openDirAbsolute(io, path, .{ .iterate = true });
    defer dir.close(io);
    try dir.setPermissions(io, std.Io.File.Permissions.fromMode(mode));
}

fn subPath(buf: []u8, home: []const u8, name: []const u8) ![]const u8 {
    return std.fmt.bufPrint(buf, "{s}/{s}", .{ home, name });
}

test "a home whose lock file cannot be created is unavailable, never adopted" {
    // A home on a read-only mount, or one owned by another user, has to refuse
    // the claim rather than proceed unlocked. Two daemons sweeping one home is
    // exactly what the lock exists to prevent, so failing open is not an option.
    const home = try freshHome("lock-uncreatable");
    defer Dir.cwd().deleteTree(io, home) catch {};
    try setMode(home, 0o500);
    // Runs before the deleteTree above (defers unwind last-in-first-out), or the
    // temp tree would outlive the test.
    defer setMode(home, 0o700) catch {};

    try expectOutcome(.unavailable, boot(home));
}

test "a home that cannot take the sentinel is adopted, and reaps nothing" {
    // The lock file already exists, so the claim itself succeeds on a directory
    // that refuses new entries — and then the sentinel cannot be written. A home
    // we cannot mark is a home we do not own, so the orphan must survive: this is
    // the arm that keeps a permissions problem from being read as "safe to
    // delete everything here".
    const home = try freshHome("sentinel-unwritable");
    defer Dir.cwd().deleteTree(io, home) catch {};
    try makeDir(home, LEASE_A);
    {
        var dir = try Dir.openDirAbsolute(io, home, .{});
        defer dir.close(io);
        var lock = try dir.createFile(io, LOCK_NAME, .{ .truncate = false });
        lock.close(io);
    }
    try setMode(home, 0o500);
    defer setMode(home, 0o700) catch {};

    try expectOutcome(.adopted, boot(home));
    try std.testing.expect(exists(home, LEASE_A));
}

test "an orphan that refuses to be reaped is logged and the sweep continues past it" {
    // deleteTree cannot empty a workspace whose own directory refuses writes.
    // One such entry must not strand the others: the sweep exists to clear what
    // an unclean shutdown left behind, and stopping at the first refusal leaves
    // the disk filling up for the same reason it was filling up before.
    const home = try freshHome("reap-refused");
    defer Dir.cwd().deleteTree(io, home) catch {};
    var buf: [std.fs.max_path_bytes]u8 = undefined;
    const stuck_workspace = try subPath(&buf, home, LEASE_A);

    try makeDir(home, LEASE_A);
    try makeDir(home, LEASE_B);
    {
        var workspace = try Dir.openDirAbsolute(io, stuck_workspace, .{});
        defer workspace.close(io);
        var held = try workspace.createFile(io, "held", .{});
        held.close(io);
    }
    try setMode(stuck_workspace, 0o500);
    defer setMode(stuck_workspace, 0o700) catch {};

    try expectOutcome(.adopted, boot(home)); // sentinel now present
    // The reapable one still goes, and the count reports only what actually went.
    try expectReaped(1, boot(home));
    try std.testing.expect(exists(home, LEASE_A));
    try std.testing.expect(!exists(home, LEASE_B));
}
