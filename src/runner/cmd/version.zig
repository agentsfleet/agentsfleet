//! `zombie-runner --version` — prints the build version + git commit.
//!
//! The version string is single-sourced: `build_runner.zig` reads the repo
//! VERSION file (kept in sync by `make sync-version`) into the `build_options`
//! module, and this is the only reader. The output deliberately contains the
//! bare version number so `deploy.sh`'s `is_already_installed()` version-skip
//! (`current == *"${VERSION#v}"*`) matches and the idempotent install fires.

const std = @import("std");
const build_options = @import("build_options");

/// Format the version line into `buf`. Pure (no I/O) so the contract is
/// unit-testable. Shape: `zombie-runner <version> (git <sha>)`.
pub fn line(buf: []u8) []const u8 {
    return std.fmt.bufPrint(buf, "zombie-runner {s} (git {s})\n", .{
        build_options.version, build_options.git_commit,
    }) catch "zombie-runner\n";
}

/// Write the version line to stdout; returns the process exit code (0, or 1 if
/// the write itself fails — e.g. a closed pipe).
pub fn run() u8 {
    var buf: [128]u8 = undefined;
    std.fs.File.stdout().writeAll(line(&buf)) catch return 1;
    return 0;
}

test "version line carries the bare build version (deploy.sh idempotency contract)" {
    var buf: [128]u8 = undefined;
    const out = line(&buf);
    try std.testing.expect(std.mem.startsWith(u8, out, "zombie-runner "));
    // deploy.sh greps for the VERSION substring — it must be present verbatim.
    try std.testing.expect(std.mem.indexOf(u8, out, build_options.version) != null);
}
