//! RepoFetchTarget.zig — the daemon-owned directory a repository fetch lands
//! in, and the three proofs that make it ours.
//!
//! The fetch hook runs AFTER the child has started, and Landlock permits the
//! child to create anything it likes inside its own workspace — including a
//! `repo` symlink pointing out of the tree. A daemon that opened `{workspace}/
//! repo` by name would then fetch through that link, writing a repository (and
//! whatever `git checkout` puts beside it) wherever the child aimed it. So the
//! target is never opened by name: it is created, opened, and proved, and every
//! later step runs relative to the handle this type holds.
//!
//!   1. EXCLUSIVE — `createDir` is the claim. `mkdir(2)` returns EEXIST for a
//!      directory, a file, AND a dangling symlink, so a child that pre-created
//!      anything at that name loses the fetch rather than redirecting it. The
//!      refusal is honest: this lease gets no repository.
//!   2. NO-FOLLOW — the open sets `follow_symlinks = false`, so a child that
//!      wins the race between the create and the open (rmdir, then symlink)
//!      meets `O_NOFOLLOW` and the open fails. Directories cannot be hardlinked
//!      and an unprivileged child cannot mount, so those are the only two swaps
//!      available to it, and both are closed.
//!   3. BENEATH — the opened handle's canonical path must sit directly under the
//!      workspace's canonical path. This is the property the spec names
//!      ("resolved beneath-only"), and it RUNS on every claim; its refusal branch
//!      is unreachable only for as long as (1) and (2) both hold, which is
//!      precisely why it is kept — it is the regression guard for them, in the
//!      same posture as every other fail-closed default in this daemon. The rule
//!      itself is pure and pinned by a test at the bottom of this file.
//!
//! `StorageHome.zig` is the sibling idiom — a handle-relative, kind-checked
//! claim on a directory an untrusted party shares. What differs is the adversary:
//! there it is a stray operator value, here it is the running child.

const RepoFetchTarget = @This();

/// The lease's workspace, opened once. Held for the target's lifetime so the
/// beneath-check compares against a directory that cannot be swapped afterwards.
workspace: Dir,
/// The fetch target itself. Every git step runs with this handle as its working
/// directory, so no step ever re-resolves the path by name.
dir: Dir,

/// Why a target could not be claimed. Each variant is a distinct thing that
/// went wrong, because the child reformulates against the reason and an operator
/// reads it in the activity stream (RULE ECL — a claim that could not be made
/// because the child squatted the name is not the same failure as a missing
/// workspace).
pub const Refusal = enum {
    /// The lease's workspace could not be opened or canonicalized at all.
    workspace_unavailable,
    /// Something already exists at the target name — the child squatted it.
    target_occupied,
    /// Created, but the handle could not be opened (a symlink swap meets
    /// `O_NOFOLLOW` here) or its canonical path could not be read.
    target_unopenable,
    /// The opened handle does not resolve beneath the lease's own workspace.
    target_escaped,

    /// A short, stable reason for the child's tool result and the log. Named
    /// rather than `@tagName` so the wire words are greppable (RULE UFS).
    pub fn reason(self: Refusal) []const u8 {
        return switch (self) {
            .workspace_unavailable => "lease workspace is unavailable",
            .target_occupied => "fetch target already exists in the workspace",
            .target_unopenable => "fetch target could not be opened as a real directory",
            .target_escaped => "fetch target does not resolve beneath the lease workspace",
        };
    }
};

pub const Claim = union(enum) {
    claimed: RepoFetchTarget,
    refused: Refusal,
};

/// The workspace-relative name every fetch lands under. Shared verbatim with the
/// child (it is told where the tree is, never asked where to put it) and with
/// the tests, so one spelling reaches all three (RULE UFS).
pub const DIR_NAME = "repo";

/// Claim `{workspace_path}/repo` for this lease's fetch. `workspace_path` is
/// daemon-derived from `lease_id` and is the only name resolved here; everything
/// after runs relative to the returned handles.
///
/// Caller owns the claim and must `close` it. Never fails the lease: every
/// refusal is a named value the caller reports.
pub fn claim(io: Io, workspace_path: []const u8) Claim {
    var workspace = Dir.openDirAbsolute(io, workspace_path, .{}) catch |err| {
        log.warn("fetch_workspace_open_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .path = workspace_path, .err = @errorName(err) });
        return .{ .refused = .workspace_unavailable };
    };
    errdefer workspace.close(io);

    var workspace_buf: [std.fs.max_path_bytes]u8 = undefined;
    const workspace_len = workspace.realPath(io, &workspace_buf) catch |err| {
        log.warn("fetch_workspace_canonicalize_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .path = workspace_path, .err = @errorName(err) });
        workspace.close(io);
        return .{ .refused = .workspace_unavailable };
    };

    // The claim. EEXIST here means the child got there first — refuse rather
    // than reuse, because a directory we did not create is one we cannot vouch
    // for, and a reused one could already hold planted content.
    workspace.createDir(io, DIR_NAME, .default_dir) catch |err| {
        log.warn("fetch_target_create_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .path = workspace_path, .err = @errorName(err) });
        workspace.close(io);
        return .{ .refused = .target_occupied };
    };

    var dir = workspace.openDir(io, DIR_NAME, .{ .iterate = true, .follow_symlinks = false }) catch |err| {
        log.warn("fetch_target_open_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .path = workspace_path, .err = @errorName(err) });
        workspace.close(io);
        return .{ .refused = .target_unopenable };
    };
    errdefer dir.close(io);

    var target_buf: [std.fs.max_path_bytes]u8 = undefined;
    const target_len = dir.realPath(io, &target_buf) catch |err| {
        log.warn("fetch_target_canonicalize_failed", .{ .error_code = ERR_EXEC_RUNNER_FLEET_INIT, .path = workspace_path, .err = @errorName(err) });
        dir.close(io);
        workspace.close(io);
        return .{ .refused = .target_unopenable };
    };

    if (!isBeneath(workspace_buf[0..workspace_len], target_buf[0..target_len])) {
        log.warn("fetch_target_escaped", .{
            .error_code = ERR_EXEC_RUNNER_FLEET_INIT,
            .workspace = workspace_buf[0..workspace_len],
            .target = target_buf[0..target_len],
        });
        dir.close(io);
        workspace.close(io);
        return .{ .refused = .target_escaped };
    }

    return .{ .claimed = .{ .workspace = workspace, .dir = dir } };
}

/// Release both handles. The fetched tree itself is NOT removed — it is the
/// product, and the per-lease `deleteTree` at run end (plus the startup sweep in
/// `daemon/StorageHome.zig`) is what reclaims it.
pub fn close(self: *RepoFetchTarget, io: Io) void {
    self.dir.close(io);
    self.workspace.close(io);
    // SAFETY: both descriptors are closed above, so every field is spent. Poisoning
    // makes a use-after-close trap instead of reusing a stale handle number the
    // kernel may already have handed to something else (RULE A5).
    self.* = undefined;
}

/// True when `target` is the immediate child of `workspace`, both canonical.
/// Immediate rather than merely-underneath: the fetch lands at exactly one known
/// name, so anything deeper is as wrong as anything outside.
fn isBeneath(workspace: []const u8, target: []const u8) bool {
    if (target.len != workspace.len + 1 + DIR_NAME.len) return false;
    if (!std.mem.eql(u8, target[0..workspace.len], workspace)) return false;
    if (target[workspace.len] != PATH_SEPARATOR) return false;
    return std.mem.eql(u8, target[workspace.len + 1 ..], DIR_NAME);
}

const std = @import("std");
const logging = @import("log");
const client_errors = @import("engine/client_errors.zig");

const Io = std.Io;
const Dir = std.Io.Dir;
const log = logging.scoped(.fleet_runner);
const ERR_EXEC_RUNNER_FLEET_INIT = client_errors.ERR_EXEC_RUNNER_FLEET_INIT;

const PATH_SEPARATOR: u8 = '/';

// The beneath rule is pure, so it is pinned here beside the prose it encodes.
// Everything needing a real directory — the claim, the squat, the symlink swap —
// lives in the sibling test file.
test {
    _ = @import("repo_fetch_target_test.zig");
}

test "isBeneath accepts only the workspace's own immediate repo directory" {
    try std.testing.expect(isBeneath("/srv/run/0199a4c1", "/srv/run/0199a4c1/repo"));
    try std.testing.expect(!isBeneath("/srv/run/0199a4c1", "/srv/run/0199a4c1")); // the workspace itself
    try std.testing.expect(!isBeneath("/srv/run/0199a4c1", "/srv/run/0199a4c2/repo")); // a sibling lease
    try std.testing.expect(!isBeneath("/srv/run/0199a4c1", "/srv/run/0199a4c1/nested/repo")); // deeper
    try std.testing.expect(!isBeneath("/srv/run/0199a4c1", "/srv/run/0199a4c1/repository")); // prefix, not the name
    try std.testing.expect(!isBeneath("/srv/run/0199a4c1", "/etc/repo")); // elsewhere entirely
    // A prefix match on the WORKSPACE is the subtle one: `…c1` is a textual
    // prefix of `…c10`, so a plain startsWith would admit another lease's tree.
    try std.testing.expect(!isBeneath("/srv/run/0199a4c1", "/srv/run/0199a4c10/repo"));
}
