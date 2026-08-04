//! Tests for what actually bounds a fetch (`repo_fetch_bounds.zig`): the byte
//! measure, and the run that kills on a breach.
//!
//! These spawn real processes and write real files, because both bounds are
//! claims about a thing that is running — a measure over a fake tree and a
//! deadline over a fake child would prove neither. `/bin/sh` stands in for git:
//! the run primitive is indifferent to which program it holds, and a shell lets
//! each bound be provoked in one line.
//!
//! The tick interval is injected rather than waited out. Production re-measures
//! once a second, which is right beside a network fetch and wrong inside a test
//! suite, so every case here sets `check_interval_ms` small — the same
//! convention `RenewHook.tick_ms` already uses.

const std = @import("std");
const bounds = @import("repo_fetch_bounds.zig");

const clock = @import("common").clock;
const Dir = std.Io.Dir;
const testing = std.testing;

const SHELL = "/bin/sh";
/// Small enough that a breach is observed in tens of milliseconds.
const TEST_TICK_MS: i64 = 25;
/// Generous ceiling for the cases that must NOT stop on time, so a loaded CI box
/// cannot turn a size assertion into a timeout assertion.
const GENEROUS_DEADLINE_MS: i64 = 30_000;
/// Upper bound on how long a killed run may take to return. Far above any real
/// kill-and-reap, far below the 30s the child would sleep if nothing killed it —
/// so the assertion distinguishes "bounded" from "waited it out".
const PROMPT_RETURN_MS: i64 = 10_000;
const KIB: u64 = 1024;

/// The quota case's arithmetic, named so the relationship is readable: the child
/// writes eight times the ceiling it is held to, so the breach is unambiguous
/// even if a filesystem rounds a block or two.
const QUOTA_CEILING_BYTES: u64 = 8 * KIB;
const QUOTA_WRITE_BLOCKS: u64 = 64;
const QUOTA_WRITE_COMMAND = std.fmt.comptimePrint(
    "dd if=/dev/zero of=fat bs={d} count={d} 2>/dev/null; sleep 30",
    .{ KIB, QUOTA_WRITE_BLOCKS },
);

/// A spawn-capable IO. `common.globalIo()` carries a `.failing` allocator — fine
/// for the sync/sleep seam, but `std.process.spawn` allocates the argv/envp block
/// before fork, so it must reach a real gpa or every spawn errors. Production
/// uses the process `Init.io`; tests stand up their own. Caller owns `t`.
fn spawnIo(t: *std.Io.Threaded) std.Io {
    t.* = .init(std.testing.allocator, .{});
    return t.io();
}

fn freshDir(io: std.Io, comptime name: []const u8) ![]const u8 {
    const path = "/tmp/agentsfleet-rfb-test-" ++ name;
    try Dir.cwd().deleteTree(io, path);
    try Dir.createDirAbsolute(io, path, .default_dir);
    return path;
}

/// A minimal environment: the run replaces the child's environ wholesale, so a
/// shell with no `PATH` could not find the tools these cases use.
fn shellEnviron(alloc: std.mem.Allocator) !std.process.Environ.Map {
    var env: std.process.Environ.Map = .init(alloc);
    errdefer env.deinit();
    try env.put("PATH", "/usr/bin:/bin");
    return env;
}

fn writeBytes(io: std.Io, dir: Dir, name: []const u8, count: usize) !void {
    const file = try dir.createFile(io, name, .{});
    defer file.close(io);
    var chunk: [KIB]u8 = @splat('x');
    var written: usize = 0;
    while (written < count) {
        const n = @min(chunk.len, count - written);
        try file.writeStreamingAll(io, chunk[0..n]);
        written += n;
    }
}

fn expectBytes(expected: u64, m: bounds.Measure) !void {
    switch (m) {
        .bytes => |actual| try testing.expectEqual(expected, actual),
        .over_limit => {
            std.debug.print("expected {d} bytes, got .over_limit\n", .{expected});
            return error.TestUnexpectedResult;
        },
    }
}

test "measure totals a nested tree and never follows a symlink out of it" {
    var threaded: std.Io.Threaded = undefined;
    const io = spawnIo(&threaded);
    defer threaded.deinit();

    const root = try freshDir(io, "measure");
    defer Dir.cwd().deleteTree(io, root) catch {};
    const outside = try freshDir(io, "measure-outside");
    defer Dir.cwd().deleteTree(io, outside) catch {};
    {
        var out = try Dir.openDirAbsolute(io, outside, .{});
        defer out.close(io);
        try writeBytes(io, out, "huge", 64 * KIB); // must not be counted
    }

    var dir = try Dir.openDirAbsolute(io, root, .{ .iterate = true });
    defer dir.close(io);
    try writeBytes(io, dir, "a", 100);
    try dir.createDir(io, "nested", .default_dir);
    {
        var nested = try dir.openDir(io, "nested", .{ .iterate = true });
        defer nested.close(io);
        try writeBytes(io, nested, "b", 250);
    }
    // A link's own bytes are its target's, wherever that lives; counting them
    // here would attribute another tree's size to this one.
    try dir.symLink(io, outside, "escape", .{});

    try expectBytes(350, bounds.measure(io, dir, 1 * KIB));
}

test "measure stops at the ceiling instead of totalling the whole tree" {
    // The short-circuit is not an optimization: the walk runs on a tick beside a
    // live fetch, and the answer the caller needs is "over", not "how far over".
    var threaded: std.Io.Threaded = undefined;
    const io = spawnIo(&threaded);
    defer threaded.deinit();

    const root = try freshDir(io, "ceiling");
    defer Dir.cwd().deleteTree(io, root) catch {};
    var dir = try Dir.openDirAbsolute(io, root, .{ .iterate = true });
    defer dir.close(io);
    try writeBytes(io, dir, "a", 4 * KIB);
    try writeBytes(io, dir, "b", 4 * KIB);

    try expectBytes(8 * KIB, bounds.measure(io, dir, 16 * KIB));
    try testing.expectEqual(bounds.Measure.over_limit, bounds.measure(io, dir, 4 * KIB));

    // An empty tree is zero, not a refusal — a lease that fetches nothing pays
    // nothing, and the measure must say so rather than fail closed on absence.
    const empty = try freshDir(io, "ceiling-empty");
    defer Dir.cwd().deleteTree(io, empty) catch {};
    var empty_dir = try Dir.openDirAbsolute(io, empty, .{ .iterate = true });
    defer empty_dir.close(io);
    try expectBytes(0, bounds.measure(io, empty_dir, 1));
}

test "run reports a child's own exit status and keeps its stderr for the log" {
    var threaded: std.Io.Threaded = undefined;
    const io = spawnIo(&threaded);
    defer threaded.deinit();

    const root = try freshDir(io, "exit");
    defer Dir.cwd().deleteTree(io, root) catch {};
    var dir = try Dir.openDirAbsolute(io, root, .{ .iterate = true });
    defer dir.close(io);

    var env = try shellEnviron(testing.allocator);
    defer env.deinit();
    var buf: [256]u8 = undefined;

    const ok = try bounds.run(io, .{
        .argv = &.{ SHELL, "-c", "exit 0" },
        .environ = &env,
        .cwd = dir,
        .target = dir,
        .bounds = .{
            .deadline_ms = clock.nowMillis() + GENEROUS_DEADLINE_MS,
            .max_bytes = 1 * KIB,
            .check_interval_ms = TEST_TICK_MS,
        },
    }, &buf);
    try testing.expect(ok.succeeded());
    try testing.expectEqual(@as(?u8, 0), ok.exit_code);

    // A non-zero exit is `completed`, not a bound breach: git failing to fetch is
    // a different report than git being killed for taking too long (RULE ECL).
    const bad = try bounds.run(io, .{
        .argv = &.{ SHELL, "-c", "echo denied >&2; exit 3" },
        .environ = &env,
        .cwd = dir,
        .target = dir,
        .bounds = .{
            .deadline_ms = clock.nowMillis() + GENEROUS_DEADLINE_MS,
            .max_bytes = 1 * KIB,
            .check_interval_ms = TEST_TICK_MS,
        },
    }, &buf);
    try testing.expect(!bad.succeeded());
    try testing.expectEqual(bounds.Stop.completed, bad.stop);
    try testing.expectEqual(@as(?u8, 3), bad.exit_code);
    try testing.expect(std.mem.indexOf(u8, bad.stderr, "denied") != null);
}

test "run kills a child that outlives the deadline, and returns promptly" {
    var threaded: std.Io.Threaded = undefined;
    const io = spawnIo(&threaded);
    defer threaded.deinit();

    const root = try freshDir(io, "deadline");
    defer Dir.cwd().deleteTree(io, root) catch {};
    var dir = try Dir.openDirAbsolute(io, root, .{ .iterate = true });
    defer dir.close(io);

    var env = try shellEnviron(testing.allocator);
    defer env.deinit();
    var buf: [64]u8 = undefined;

    const started = clock.nowMillis();
    const out = try bounds.run(io, .{
        .argv = &.{ SHELL, "-c", "sleep 30" },
        .environ = &env,
        .cwd = dir,
        .target = dir,
        .bounds = .{ .deadline_ms = started + 200, .max_bytes = 1 * KIB, .check_interval_ms = TEST_TICK_MS },
    }, &buf);
    try testing.expectEqual(bounds.Stop.timed_out, out.stop);
    try testing.expectEqual(@as(?u8, null), out.exit_code);
    // The point of killing before the wait: the wait cannot be what returns. A
    // deadline checked after `child.wait()` would have blocked the full 30s here.
    try testing.expect(clock.nowMillis() - started < PROMPT_RETURN_MS);
}

test "test_fetch_is_bounded_by_bytes_not_history" {
    // Depth bounds HISTORY; this bounds BYTES. One commit can carry arbitrarily
    // large blobs, `disk_write_limit_mb` is unenforced in `CgroupScope`, and this
    // run happens outside the child's cgroup — so without the measure, nothing
    // sits between a fetch and a full host disk. The child here writes past the
    // ceiling and then lingers, exactly as a runaway fetch would.
    var threaded: std.Io.Threaded = undefined;
    const io = spawnIo(&threaded);
    defer threaded.deinit();

    const root = try freshDir(io, "quota");
    defer Dir.cwd().deleteTree(io, root) catch {};
    var dir = try Dir.openDirAbsolute(io, root, .{ .iterate = true });
    defer dir.close(io);

    var env = try shellEnviron(testing.allocator);
    defer env.deinit();
    var buf: [256]u8 = undefined;

    const started = clock.nowMillis();
    const out = try bounds.run(io, .{
        .argv = &.{ SHELL, "-c", QUOTA_WRITE_COMMAND },
        .environ = &env,
        .cwd = dir,
        .target = dir,
        .bounds = .{
            .deadline_ms = started + GENEROUS_DEADLINE_MS,
            .max_bytes = QUOTA_CEILING_BYTES,
            .check_interval_ms = TEST_TICK_MS,
        },
    }, &buf);

    try testing.expectEqual(bounds.Stop.over_quota, out.stop);
    // Stopped by the SIZE bound, with the time bound nowhere near — the two are
    // separately load-bearing, and a test that let them blur would pass on either.
    try testing.expect(clock.nowMillis() - started < PROMPT_RETURN_MS);
}
