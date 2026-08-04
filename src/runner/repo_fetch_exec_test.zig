//! End-to-end tests for the fetch execution half (`repo_fetch_exec.zig`),
//! against a local `file://` remote.
//!
//! No network and no credential are needed to prove any of this, which is the
//! useful discovery: a `git init` fixture in a temp directory speaks the same
//! transport GitHub does, so the depth bound, the refs, the working tree, and
//! the credential's absence are all observable here rather than on a stage.
//!
//! The fixture carries SIX commits and three arrive, so "history was bounded" is
//! a count that differs from the full history rather than a flag that was
//! passed. And the revert the repairer would run is actually run at the end: it
//! is the only assertion that proves the suspect commit's PARENT came down too,
//! which is the whole reason the depth is 2 and not 1.
//!
//! Skips when the host has no git — the runner needs it in production (§7
//! installs it), but a developer machine without it should not fail the suite.

const std = @import("std");
const exec = @import("repo_fetch_exec.zig");
const RepoFetchTarget = @import("RepoFetchTarget.zig");

const clock = @import("common").clock;
const Dir = std.Io.Dir;
const testing = std.testing;

/// The repository the fixture stands in for. Only the SHAPE matters here — the
/// binding check that ties this name to the ask is `repo_fetch.decide`'s, and it
/// is unit-tested with no filesystem at all.
const REPOSITORY = "acme/payments";
/// Deliberately not token-shaped: this string is scanned FOR, so a realistic
/// spelling would be a secret-scanner finding on every commit that carries it.
const FAKE_TOKEN = "fetch-test-credential-must-not-persist";

/// Commits on the fixture remote. Each adds ITS OWN file, so reverting one
/// applies cleanly onto a head that moved past it — a commit that rewrote the
/// same line every time would conflict, which is Dimension 4.5's case and a
/// different test.
const FIXTURE_COMMITS: usize = 6;
/// The suspect: not the tip, so the revert is a real three-way merge onto a head
/// that has moved.
const SUSPECT_COMMIT: usize = 5;
/// Commits reachable from the fetched head afterwards. Depth 2 from the head
/// (`c6, c5`) unioned with depth 2 from the suspect (`c5, c4`) leaves `c6, c5,
/// c4` on the head's ancestry — three of six, so the bound is a real cut rather
/// than a flag that happened to be passed.
const EXPECTED_HEAD_COMMITS: usize = 3;
/// Ample for a local `file://` fetch; the deadline is not what these test.
const TEST_DEADLINE_MS: i64 = 60_000;

/// A spawn-capable IO — `common.globalIo()` carries a `.failing` allocator and
/// every `std.process.spawn` under it errors. See `repo_fetch_bounds_test.zig`
/// for the full note; production uses the process `Init.io`.
fn spawnIo(t: *std.Io.Threaded) std.Io {
    t.* = .init(std.testing.allocator, .{});
    return t.io();
}

fn freshDir(io: std.Io, comptime name: []const u8) ![]const u8 {
    const path = "/tmp/agentsfleet-rfe-test-" ++ name;
    try Dir.cwd().deleteTree(io, path);
    try Dir.createDirAbsolute(io, path, .default_dir);
    return path;
}

/// git's environment for the FIXTURE side. Hermetic for the same reason the
/// production build is: whatever the developer has in `~/.gitconfig` must not
/// decide whether this test passes.
fn fixtureEnviron(alloc: std.mem.Allocator) !std.process.Environ.Map {
    var env: std.process.Environ.Map = .init(alloc);
    errdefer env.deinit();
    try env.put("PATH", "/usr/bin:/bin");
    try env.put("GIT_CONFIG_GLOBAL", "/dev/null");
    try env.put("GIT_CONFIG_SYSTEM", "/dev/null");
    try env.put("GIT_TERMINAL_PROMPT", "0");
    try env.put("GIT_AUTHOR_NAME", "fixture");
    try env.put("GIT_AUTHOR_EMAIL", "fixture@example.invalid");
    try env.put("GIT_COMMITTER_NAME", "fixture");
    try env.put("GIT_COMMITTER_EMAIL", "fixture@example.invalid");
    return env;
}

/// Run one git command in `cwd`, returning its trimmed stdout (caller frees).
/// Fails the test on a non-zero exit, with git's stderr in the message — a
/// fixture that silently half-built would make every later assertion a lie.
fn git(io: std.Io, alloc: std.mem.Allocator, git_bin: []const u8, cwd: []const u8, args: []const []const u8) ![]u8 {
    var argv = try std.ArrayList([]const u8).initCapacity(alloc, args.len + 1);
    defer argv.deinit(alloc);
    argv.appendAssumeCapacity(git_bin);
    for (args) |a| argv.appendAssumeCapacity(a);

    var env = try fixtureEnviron(alloc);
    defer env.deinit();

    var dir = try Dir.openDirAbsolute(io, cwd, .{});
    defer dir.close(io);

    const result = try std.process.run(alloc, io, .{
        .argv = argv.items,
        .cwd = .{ .dir = dir },
        .environ_map = &env,
    });
    defer alloc.free(result.stderr);
    defer alloc.free(result.stdout);
    if (result.term != .exited or result.term.exited != 0) {
        std.debug.print("fixture git {s} failed: {s}\n", .{ args[0], result.stderr });
        return error.TestUnexpectedResult;
    }
    return alloc.dupe(u8, std.mem.trim(u8, result.stdout, " \t\r\n"));
}

/// Run one git command for its side effect only, discarding its output.
fn gitVoid(io: std.Io, alloc: std.mem.Allocator, git_bin: []const u8, cwd: []const u8, args: []const []const u8) !void {
    alloc.free(try git(io, alloc, git_bin, cwd, args));
}

/// The file commit `n` adds. One file per commit keeps the revert clean.
fn changeFile(buf: []u8, n: usize) ![]const u8 {
    return std.fmt.bufPrint(buf, "change-{d}.conf", .{n});
}

/// A remote with `FIXTURE_COMMITS` commits on its default branch. Returns the
/// object id of `SUSPECT_COMMIT` (caller frees).
fn buildFixture(io: std.Io, alloc: std.mem.Allocator, git_bin: []const u8, path: []const u8) ![]u8 {
    try gitVoid(io, alloc, git_bin, path, &.{ "init", "--quiet", "--initial-branch=main" });
    // A want-by-object-id over the wire is what the daemon asks for, and GitHub
    // serves it for a reachable commit. The fixture says yes for the same reason.
    try gitVoid(io, alloc, git_bin, path, &.{ "config", "uploadpack.allowAnySHA1InWant", "true" });

    var suspect: ?[]u8 = null;
    errdefer if (suspect) |s| alloc.free(s);
    for (1..FIXTURE_COMMITS + 1) |n| {
        var name_buf: [32]u8 = undefined;
        const name = try changeFile(&name_buf, n);
        {
            var dir = try Dir.openDirAbsolute(io, path, .{});
            defer dir.close(io);
            try dir.writeFile(io, .{ .sub_path = name, .data = name });
        }
        try gitVoid(io, alloc, git_bin, path, &.{ "add", name });
        try gitVoid(io, alloc, git_bin, path, &.{ "commit", "--quiet", "-m", name });
        if (n == SUSPECT_COMMIT) suspect = try git(io, alloc, git_bin, path, &.{ "rev-parse", "HEAD" });
    }
    return suspect orelse error.TestUnexpectedResult;
}

fn fileUrl(buf: []u8, path: []const u8) ![]const u8 {
    return std.fmt.bufPrint(buf, "file://{s}", .{path});
}

/// True when `needle` appears in any path or any regular file's bytes under
/// `root` — names as well as contents, because a credential written into a path
/// is just as readable to the child as one written into a config body.
fn treeContains(io: std.Io, alloc: std.mem.Allocator, root: []const u8, needle: []const u8) !bool {
    var dir = try Dir.openDirAbsolute(io, root, .{ .iterate = true });
    defer dir.close(io);
    var walker = try dir.walk(alloc);
    defer walker.deinit();
    while (try walker.next(io)) |entry| {
        if (std.mem.indexOf(u8, entry.path, needle) != null) return true;
        if (entry.kind != .file) continue;
        const body = entry.dir.readFileAlloc(io, entry.basename, alloc, .limited(MAX_SCANNED_FILE_BYTES)) catch continue;
        defer alloc.free(body);
        if (std.mem.indexOf(u8, body, needle) != null) return true;
    }
    return false;
}

const MAX_SCANNED_FILE_BYTES: usize = 4 * 1024 * 1024;

/// One fixture remote plus one lease workspace, torn down together.
const Fixture = struct {
    io: std.Io,
    git_bin: []const u8,
    remote_path: []const u8,
    workspace: []const u8,
    suspect: []u8,
    alloc: std.mem.Allocator,

    /// A failure part-way through leaves its temp directories behind on purpose:
    /// `freshDir` deletes any stale tree before creating, so the next run starts
    /// clean either way, and an abandoned fixture is worth reading when a test
    /// has just failed.
    fn init(io: std.Io, alloc: std.mem.Allocator, comptime name: []const u8) !Fixture {
        const git_bin = exec.gitPath(io) orelse return error.SkipZigTest;
        const remote_path = try freshDir(io, name ++ "-remote");
        const workspace = try freshDir(io, name ++ "-ws");
        return .{
            .io = io,
            .git_bin = git_bin,
            .remote_path = remote_path,
            .workspace = workspace,
            .suspect = try buildFixture(io, alloc, git_bin, remote_path),
            .alloc = alloc,
        };
    }

    /// Frees only what is owned in memory. The two temp directories are torn
    /// down by the test that made them (`defer …deleteTree`), the way every
    /// sibling filesystem suite does it.
    fn deinit(self: *Fixture) void {
        self.alloc.free(self.suspect);
    }

    /// Absolute path of the fetched tree — where the child finds its worktree.
    fn targetPath(self: Fixture, buf: []u8) ![]const u8 {
        return std.fmt.bufPrint(buf, "{s}/{s}", .{ self.workspace, RepoFetchTarget.DIR_NAME });
    }

    fn run(self: Fixture, args: []const []const u8) ![]u8 {
        var buf: [std.fs.max_path_bytes]u8 = undefined;
        return git(self.io, self.alloc, self.git_bin, try self.targetPath(&buf), args);
    }

    /// Run the fetch under test against the fixture remote.
    fn fetch(self: Fixture, token: []const u8) !exec.Outcome {
        var url_buf: [std.fs.max_path_bytes]u8 = undefined;
        return self.fetchFrom(try fileUrl(&url_buf, self.remote_path), token, clock.nowMillis() + TEST_DEADLINE_MS);
    }

    fn fetchFrom(self: Fixture, url: []const u8, token: []const u8, deadline_ms: i64) exec.Outcome {
        return exec.fetch(self.io, self.alloc, .{
            .workspace_path = self.workspace,
            .approved = .{
                .repository = REPOSITORY,
                .commit = self.suspect,
                .head = "",
                .access = .write,
            },
            .remote_url = url,
            .token = token,
            .deadline_ms = deadline_ms,
        });
    }
};

fn expectReady(outcome: exec.Outcome) !void {
    switch (outcome) {
        .ready => {},
        .failed => |f| {
            std.debug.print("expected a ready tree, got: {s}\n", .{f.reason()});
            return error.TestUnexpectedResult;
        },
    }
}

fn expectFailure(expected: exec.Failure, outcome: exec.Outcome) !void {
    switch (outcome) {
        .ready => {
            std.debug.print("expected failure .{s}, got a ready tree\n", .{@tagName(expected)});
            return error.TestUnexpectedResult;
        },
        .failed => |actual| try testing.expectEqual(expected, actual),
    }
}

test "test_fetch_is_bounded_and_credential_free" {
    // Dimension 4.6, both halves.
    const alloc = testing.allocator;
    var threaded: std.Io.Threaded = undefined;
    const io = spawnIo(&threaded);
    defer threaded.deinit();

    var fx = try Fixture.init(io, alloc, "bounded");
    defer fx.deinit();
    defer Dir.cwd().deleteTree(io, fx.remote_path) catch {};
    defer Dir.cwd().deleteTree(io, fx.workspace) catch {};

    try expectReady(try fx.fetch(FAKE_TOKEN));

    var target_buf: [std.fs.max_path_bytes]u8 = undefined;
    const target = try fx.targetPath(&target_buf);

    // BOUNDED — the remote has six commits and three arrived. A count that
    // differs from the full history is the proof; a flag having been passed is not.
    const depth = try fx.run(&.{ "rev-list", "--count", "refs/agentsfleet/head" });
    defer alloc.free(depth);
    try testing.expectEqual(EXPECTED_HEAD_COMMITS, try std.fmt.parseInt(usize, depth, 10));
    try testing.expect(EXPECTED_HEAD_COMMITS < FIXTURE_COMMITS); // the bound is a real cut

    // A shallow marker is git's own record that history was cut, not ours.
    {
        var dir = try Dir.openDirAbsolute(io, target, .{});
        defer dir.close(io);
        _ = try dir.statFile(io, ".git/shallow", .{});
    }

    // The suspect landed under its own ref, at exactly the object asked for —
    // the ask named one immutable commit and one is what arrived.
    const suspect = try fx.run(&.{ "rev-parse", "refs/agentsfleet/suspect" });
    defer alloc.free(suspect);
    try testing.expectEqualStrings(fx.suspect, suspect);

    // CREDENTIAL-FREE — the token authenticated the fetch from the git process's
    // environment, so nothing under the tree the child inherits carries it, in
    // either the raw or the wire (base64) spelling (Invariant 9).
    try testing.expect(!try treeContains(io, alloc, target, FAKE_TOKEN));
    var encoded_buf: [128]u8 = undefined;
    const encoded = std.base64.standard.Encoder.encode(&encoded_buf, FAKE_TOKEN);
    try testing.expect(!try treeContains(io, alloc, target, encoded));

    // And it landed in the lease's own workspace and nowhere else: the workspace
    // holds exactly the one directory the daemon created.
    var ws = try Dir.openDirAbsolute(io, fx.workspace, .{ .iterate = true });
    defer ws.close(io);
    var it = ws.iterate();
    const first = (try it.next(io)) orelse return error.TestUnexpectedResult;
    try testing.expectEqualStrings(RepoFetchTarget.DIR_NAME, first.name);
    try testing.expect((try it.next(io)) == null);
}

test "the fetched tree is a working tree the repairer can actually revert in" {
    // The reason the depth is 2 and not 1, made observable: `git revert` needs
    // the suspect's PARENT to compute the inverse patch. Running the repairer's
    // own command is the only assertion that proves the parent came down — and it
    // proves the checkout produced a real working tree, not a bare repository.
    const alloc = testing.allocator;
    var threaded: std.Io.Threaded = undefined;
    const io = spawnIo(&threaded);
    defer threaded.deinit();

    var fx = try Fixture.init(io, alloc, "revertable");
    defer fx.deinit();
    defer Dir.cwd().deleteTree(io, fx.remote_path) catch {};
    defer Dir.cwd().deleteTree(io, fx.workspace) catch {};

    try expectReady(try fx.fetch(""));

    var target_buf: [std.fs.max_path_bytes]u8 = undefined;
    const target = try fx.targetPath(&target_buf);
    var dir = try Dir.openDirAbsolute(io, target, .{});
    defer dir.close(io);

    var suspect_name: [32]u8 = undefined;
    var tip_name: [32]u8 = undefined;
    const suspect_file = try changeFile(&suspect_name, SUSPECT_COMMIT);
    const tip_file = try changeFile(&tip_name, FIXTURE_COMMITS);

    // The checkout produced a real working tree at the HEAD, not at the suspect.
    _ = try dir.statFile(io, tip_file, .{});
    _ = try dir.statFile(io, suspect_file, .{});

    // The revert itself — no network, no credential, no model.
    try gitVoid(io, alloc, fx.git_bin, target, &.{ "revert", "--no-commit", "refs/agentsfleet/suspect" });

    // The suspect's change is gone and every later commit's survives, which is
    // only computable with the suspect's PARENT present — the reason depth is 2.
    try testing.expectError(error.FileNotFound, dir.statFile(io, suspect_file, .{}));
    _ = try dir.statFile(io, tip_file, .{});
}

test "an unreachable remote fails with a named reason and leaves no partial tree" {
    // Spec Failure Modes: "no partial tree is left for a later step to misread".
    // The claimed directory exists (the daemon made it), but nothing in it can be
    // mistaken for a repository to revert in, and the reason says which step lost.
    const alloc = testing.allocator;
    var threaded: std.Io.Threaded = undefined;
    const io = spawnIo(&threaded);
    defer threaded.deinit();

    var fx = try Fixture.init(io, alloc, "unreachable");
    defer fx.deinit();
    defer Dir.cwd().deleteTree(io, fx.remote_path) catch {};
    defer Dir.cwd().deleteTree(io, fx.workspace) catch {};

    var url_buf: [std.fs.max_path_bytes]u8 = undefined;
    const url = try fileUrl(&url_buf, "/tmp/agentsfleet-rfe-test-no-such-remote");
    try expectFailure(.fetch_failed, fx.fetchFrom(url, "", clock.nowMillis() + TEST_DEADLINE_MS));

    var target_buf: [std.fs.max_path_bytes]u8 = undefined;
    var dir = try Dir.openDirAbsolute(io, try fx.targetPath(&target_buf), .{});
    defer dir.close(io);
    // `git init` succeeded, so a `.git` exists — but neither fetched ref does, and
    // no worktree file does, so nothing downstream reads this as a populated tree.
    var tip_name: [32]u8 = undefined;
    try testing.expectError(error.FileNotFound, dir.statFile(io, ".git/refs/agentsfleet/head", .{}));
    try testing.expectError(error.FileNotFound, dir.statFile(io, try changeFile(&tip_name, FIXTURE_COMMITS), .{}));
}

test "a fetch whose time budget has already elapsed produces no tree" {
    // The deadline is absolute and shared by all three steps, so a lease with no
    // time left never completes one — the same fail-closed posture as a lease
    // with no binding never starting a fetch at all.
    const alloc = testing.allocator;
    var threaded: std.Io.Threaded = undefined;
    const io = spawnIo(&threaded);
    defer threaded.deinit();

    var fx = try Fixture.init(io, alloc, "expired");
    defer fx.deinit();
    defer Dir.cwd().deleteTree(io, fx.remote_path) catch {};
    defer Dir.cwd().deleteTree(io, fx.workspace) catch {};

    var url_buf: [std.fs.max_path_bytes]u8 = undefined;
    const url = try fileUrl(&url_buf, fx.remote_path);
    try expectFailure(.timed_out, fx.fetchFrom(url, "", clock.nowMillis() - 1));
}

test "every failure reason is distinct and readable" {
    // The child reformulates against these and an operator reads them in the
    // activity stream, so a blank or duplicated reason is a real defect.
    const all = std.enums.values(exec.Failure);
    for (all, 0..) |a, i| {
        try testing.expect(a.reason().len > 0);
        for (all[i + 1 ..]) |b| {
            try testing.expect(!std.mem.eql(u8, a.reason(), b.reason()));
        }
    }
}
