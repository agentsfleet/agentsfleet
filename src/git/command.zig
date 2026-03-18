//! Low-level child-process execution for git and curl commands.
//!
//! Exports:
//!   - `CommandResources` — RAII wrapper around `std.process.Child` with
//!     stdout/stderr capture.  Caller must call `deinit` to release buffers
//!     and the env map.
//!   - `sanitizedChildEnv` / `copyEnvIfPresent` — build a minimal env map
//!     that strips any credential or git-state variables.
//!   - `run` — spawn, wait, capture stdout; returns caller-owned slice.
//!
//! Ownership:
//!   - `CommandResources` owns `stdout`, `stderr`, and `env`.  All are freed
//!     in `deinit`.
//!   - `run` returns a caller-owned slice; the caller must `alloc.free` it.

const std = @import("std");
const GitError = @import("errors.zig").GitError;
const log = std.log.scoped(.git);

pub const CommandResources = struct {
    child: std.process.Child,
    alloc: std.mem.Allocator,
    env: std.process.EnvMap,
    stdout: ?[]u8 = null,
    stderr: ?[]u8 = null,

    pub fn init(
        alloc: std.mem.Allocator,
        argv: []const []const u8,
        cwd: ?[]const u8,
    ) !CommandResources {
        var child = std.process.Child.init(argv, alloc);
        child.cwd = cwd;
        child.stdout_behavior = .Pipe;
        child.stderr_behavior = .Pipe;
        var env = try sanitizedChildEnv(alloc);
        errdefer env.deinit();
        child.env_map = &env;
        try child.spawn();
        return .{ .child = child, .alloc = alloc, .env = env };
    }

    pub fn readOutput(self: *CommandResources) !void {
        self.stdout = if (self.child.stdout) |*s|
            try s.readToEndAlloc(self.alloc, 1024 * 1024)
        else
            try self.alloc.dupe(u8, "");
        errdefer if (self.stdout) |b| self.alloc.free(b);

        self.stderr = if (self.child.stderr) |*s|
            try s.readToEndAlloc(self.alloc, 1024 * 1024)
        else
            try self.alloc.dupe(u8, "");
        errdefer if (self.stderr) |b| self.alloc.free(b);
    }

    pub fn takeStdout(self: *CommandResources) []const u8 {
        const out = self.stdout orelse "";
        self.stdout = null;
        return out;
    }

    pub fn stderrOrEmpty(self: *const CommandResources) []const u8 {
        return self.stderr orelse "";
    }

    pub fn deinit(self: *CommandResources) void {
        if (self.stdout) |b| self.alloc.free(b);
        if (self.stderr) |b| self.alloc.free(b);
        self.env.deinit();
    }
};

/// Build a child environment containing only the safe allowlist of variables.
/// Caller owns the returned `EnvMap`; call `.deinit()` when done.
pub fn sanitizedChildEnv(alloc: std.mem.Allocator) !std.process.EnvMap {
    var env = std.process.EnvMap.init(alloc);
    errdefer env.deinit();

    try copyEnvIfPresent(alloc, &env, "PATH");
    try copyEnvIfPresent(alloc, &env, "HOME");
    try copyEnvIfPresent(alloc, &env, "TMPDIR");
    try copyEnvIfPresent(alloc, &env, "SSL_CERT_FILE");
    try copyEnvIfPresent(alloc, &env, "SSL_CERT_DIR");
    return env;
}

/// Copy `key` from the process environment into `env` if present.
/// No-ops silently when the variable is absent.
pub fn copyEnvIfPresent(alloc: std.mem.Allocator, env: *std.process.EnvMap, key: []const u8) !void {
    const value = std.process.getEnvVarOwned(alloc, key) catch return;
    defer alloc.free(value);
    try env.put(key, value);
}

/// Spawn `argv` with `cwd`, wait for exit, and return the captured stdout
/// (caller-owned).  Returns `GitError.CommandFailed` on non-zero exit or
/// signal termination.  `timeout_ms` is accepted but not currently enforced.
pub fn run(
    alloc: std.mem.Allocator,
    argv: []const []const u8,
    cwd: ?[]const u8,
    timeout_ms: u64,
) ![]const u8 {
    var resources = try CommandResources.init(alloc, argv, cwd);
    defer resources.deinit();
    _ = timeout_ms;
    const term = try resources.child.wait();

    try resources.readOutput();
    const stderr = resources.stderrOrEmpty();

    switch (term) {
        .Exited => |code| if (code != 0) {
            log.err("command failed code={d} argv[0]={s} stderr={s}", .{ code, argv[0], stderr });
            return GitError.CommandFailed;
        },
        else => {
            return GitError.CommandFailed;
        },
    }

    return resources.takeStdout();
}

// ---------------------------------------------------------------------------
// Tests (moved from ops.zig)
// ---------------------------------------------------------------------------

test "integration: sanitizedChildEnv keeps minimal allowlist" {
    var env = try sanitizedChildEnv(std.testing.allocator);
    defer env.deinit();

    if (std.process.getEnvVarOwned(std.testing.allocator, "PATH")) |value| {
        defer std.testing.allocator.free(value);
        try std.testing.expect(env.get("PATH") != null);
    } else |_| {}
    try std.testing.expect(env.get("GIT_DIR") == null);
    try std.testing.expect(env.get("GIT_WORK_TREE") == null);
    try std.testing.expect(env.get("GITHUB_TOKEN") == null);
}
