//! `agentsfleet-runner --version` — prints the build version + git commit.
//!
//! The version string is single-sourced: `build_runner.zig` reads the repo
//! VERSION file (kept in sync by `make sync-version`) into the `build_options`
//! module, and this is the only reader. The bare version number is the second
//! whitespace-delimited field on purpose: `deploy.sh`'s `is_already_installed()`
//! reads that field and compares it to the target for exact equality, so a shape
//! change here turns every redeploy into a full reinstall.

const std = @import("std");
const build_options = @import("build_options");
const output = @import("output.zig");

/// Format the version line into `buf`. Pure (no I/O) so the contract is
/// unit-testable. Shape: `agentsfleet-runner <version> (git <sha>)`.
pub fn line(buf: []u8) []const u8 {
    return std.fmt.bufPrint(buf, "agentsfleet-runner {s} (git {s})\n", .{
        build_options.version, build_options.git_commit,
    }) catch "agentsfleet-runner\n";
}

/// Write the version line to stdout via the CLI output layer; returns the
/// process exit code (always 0 — a closed-pipe write failure is swallowed by
/// `output`, matching the other operator-CLI commands). Zig 0.16 removed
/// `std.fs.File`; `output.writeOut` owns the io-free stdout write.
pub fn run() u8 {
    var buf: [128]u8 = undefined;
    output.writeOut(line(&buf));
    return 0;
}

test "version line carries the bare build version in whitespace field 2 (deploy.sh idempotent-skip invariant)" {
    var buf: [128]u8 = undefined;
    const out = line(&buf);
    try std.testing.expect(std.mem.startsWith(u8, out, "agentsfleet-runner "));
    // deploy.sh parses whitespace field 2 for its exact-equality version skip, so
    // asserting the version merely appears somewhere is too weak: a reformat that
    // shifted it off field 2 would pass yet break every redeploy. Pin the field.
    var fields = std.mem.tokenizeAny(u8, out, " \n");
    _ = fields.next(); // "agentsfleet-runner"
    const version_field = fields.next() orelse return error.MissingVersionField;
    try std.testing.expectEqualStrings(build_options.version, version_field);
}
