//! Workspace credential setup for the executor.
//!
//! Writes tool-specific credential material into the workspace so tools can
//! authenticate without the agent ever seeing the raw credential value.
//!
//! This file is extracted from runner.zig to keep that module under 400 lines.
//! Each function corresponds to one tool's credential format. When a new tool
//! needs workspace credential setup, add a function here — zero changes to
//! runner.zig or handler.zig required.

const std = @import("std");
const logging = @import("log");

const log = logging.scoped(.runner_credentials);

/// Write git credentials into the workspace so git push/pull can authenticate.
///
/// Writes two files, both inside .git/ to stay out of the working tree:
///   - .git/credentials: the URL-encoded token for HTTPS auth
///   - .git/config: appended [credential] section pointing to the above file
///
/// The workspace is a temporary worktree deleted after the run, so these
/// credentials have run-scoped lifetime — no cleanup needed.
///
/// Called by runner.executeInner() when agent_config contains github_token.
pub fn prepareGitCredential(
    alloc: std.mem.Allocator,
    workspace_path: []const u8,
    github_token: []const u8,
) !void {
    const creds_path = try std.fs.path.join(alloc, &.{ workspace_path, ".git", "credentials" });
    defer alloc.free(creds_path);

    const creds_content = try std.fmt.allocPrint(
        alloc,
        "https://x-access-token:{s}@github.com\n",
        .{github_token},
    );
    defer alloc.free(creds_content);

    const file = try std.fs.createFileAbsolute(creds_path, .{ .truncate = true, .mode = 0o600 });
    defer file.close();
    try file.writeAll(creds_content);

    // Append the [credential] section to .git/config so git uses the file above.
    // The worktree .git/config is managed by libgit2 — we append our section only.
    const git_config_path = try std.fs.path.join(alloc, &.{ workspace_path, ".git", "config" });
    defer alloc.free(git_config_path);

    const git_config_file = std.fs.openFileAbsolute(git_config_path, .{ .mode = .read_write }) catch return;
    defer git_config_file.close();

    // Quote creds_path so paths with spaces parse correctly.
    const cred_section = try std.fmt.allocPrint(
        alloc,
        "\n[credential]\n\thelper = store --file=\"{s}\"\n",
        .{creds_path},
    );
    defer alloc.free(cred_section);

    try git_config_file.seekFromEnd(0);
    try git_config_file.writeAll(cred_section);

    log.debug("git_configured", .{ .workspace = workspace_path });
}
