//! Filesystem tests for the fetch target's claim (`RepoFetchTarget.zig`).
//!
//! Every test here owns a real temp workspace, because the thing under test is
//! a race with a process that shares that directory. The adversary is the
//! sandboxed child: Landlock gives it read-write over its own workspace, so it
//! may create anything at `repo` before the daemon's fetch hook ever runs — and
//! a daemon that opened that name would fetch through whatever it found.
//!
//! `/tmp` is a symlink to `/private/tmp` on macOS, so every claim here also
//! exercises the canonicalization the beneath-check depends on rather than
//! asserting it separately. The beneath rule itself is pure and pinned inline in
//! `RepoFetchTarget.zig`.

const std = @import("std");
const RepoFetchTarget = @import("RepoFetchTarget.zig");

const io = @import("common").globalIo();
const Dir = std.Io.Dir;

/// Fresh absolute temp workspace for one test, shaped like a lease's: a stale
/// tree from a previous run is deleted first, and the caller deletes it on exit.
fn freshWorkspace(comptime name: []const u8) ![]const u8 {
    const path = "/tmp/agentsfleet-rft-test-" ++ name;
    try Dir.cwd().deleteTree(io, path);
    try Dir.createDirAbsolute(io, path, .default_dir);
    return path;
}

fn expectRefused(expected: RepoFetchTarget.Refusal, claim: RepoFetchTarget.Claim) !void {
    switch (claim) {
        .refused => |actual| try std.testing.expectEqual(expected, actual),
        .claimed => |t| {
            var open = t;
            open.close(io);
            std.debug.print("expected refusal .{s}, but the target was claimed\n", .{@tagName(expected)});
            return error.TestUnexpectedResult;
        },
    }
}

test "a fresh lease workspace yields a usable, daemon-created target" {
    const ws = try freshWorkspace("claims");
    defer Dir.cwd().deleteTree(io, ws) catch {};

    var target = switch (RepoFetchTarget.claim(io, ws)) {
        .claimed => |t| t,
        .refused => |r| {
            std.debug.print("unexpected refusal: {s}\n", .{r.reason()});
            return error.TestUnexpectedResult;
        },
    };
    defer target.close(io);

    // The handle is real and writable — the fetch runs every git step against
    // exactly this fd, never against the path again.
    (try target.dir.createFile(io, "probe", .{})).close(io);
    _ = try target.workspace.statFile(io, RepoFetchTarget.DIR_NAME, .{ .follow_symlinks = false });
}

test "a child that squats the target name loses the fetch rather than redirecting it" {
    // Three squats, one per kind the child can create. All three are EEXIST to
    // `mkdir(2)`, so all three refuse — including the dangling symlink, which is
    // the one that would otherwise have redirected the whole fetch.
    const cases = [_][]const u8{ "dir", "file", "symlink" };
    inline for (cases) |kind| {
        const ws = try freshWorkspace("squat-" ++ kind);
        defer Dir.cwd().deleteTree(io, ws) catch {};
        {
            var dir = try Dir.openDirAbsolute(io, ws, .{});
            defer dir.close(io);
            if (comptime std.mem.eql(u8, kind, "dir")) {
                try dir.createDir(io, RepoFetchTarget.DIR_NAME, .default_dir);
            } else if (comptime std.mem.eql(u8, kind, "file")) {
                (try dir.createFile(io, RepoFetchTarget.DIR_NAME, .{})).close(io);
            } else {
                // Pointed at a real directory outside the workspace: if the claim
                // followed it, `git init` + `git checkout` would land there.
                try dir.symLink(io, "/tmp", RepoFetchTarget.DIR_NAME, .{});
            }
        }
        try expectRefused(.target_occupied, RepoFetchTarget.claim(io, ws));
    }
}

test "a squatting symlink is refused with its target left untouched" {
    // The consequence the refusal exists to prevent, asserted directly: nothing
    // is written through the link, so the directory it points at is unchanged.
    const ws = try freshWorkspace("no-write-through");
    defer Dir.cwd().deleteTree(io, ws) catch {};
    const outside = try freshWorkspace("no-write-through-target");
    defer Dir.cwd().deleteTree(io, outside) catch {};
    {
        var dir = try Dir.openDirAbsolute(io, ws, .{});
        defer dir.close(io);
        try dir.symLink(io, outside, RepoFetchTarget.DIR_NAME, .{});
    }

    try expectRefused(.target_occupied, RepoFetchTarget.claim(io, ws));

    // The link's target holds nothing the claim could have put there.
    var target = try Dir.openDirAbsolute(io, outside, .{ .iterate = true });
    defer target.close(io);
    var it = target.iterate();
    try std.testing.expect((try it.next(io)) == null);
}

test "a workspace that does not exist is named as such, not as a squat" {
    // The two refusals must stay distinguishable: a missing workspace is a
    // daemon-side problem and a squat is a child-side one, and an operator
    // reading the activity stream needs to know which (RULE ECL).
    try expectRefused(.workspace_unavailable, RepoFetchTarget.claim(io, "/tmp/agentsfleet-rft-test-absent-workspace"));
}

test "a second claim on the same workspace refuses instead of reusing the tree" {
    // A retried fetch within one lease must not silently inherit a half-built
    // tree from the attempt before it — that tree is exactly the "partial tree
    // left for a later step to misread" the spec's Failure Modes rule out.
    const ws = try freshWorkspace("second-claim");
    defer Dir.cwd().deleteTree(io, ws) catch {};

    var first = switch (RepoFetchTarget.claim(io, ws)) {
        .claimed => |t| t,
        .refused => return error.TestUnexpectedResult,
    };
    first.close(io);

    try expectRefused(.target_occupied, RepoFetchTarget.claim(io, ws));
}
