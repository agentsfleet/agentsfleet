const std = @import("std");
const builtin = @import("builtin");

/// Fail compilation (with a clear message) if the building Zig toolchain doesn't
/// match the version pinned in build.zig.zon (`minimum_zig_version`). agentsfleet
/// pins an exact major.minor (the toolchain is mise-pinned), so a drifted Zig
/// fails fast + legibly here instead of as a deep, cryptic error later.
pub fn requireZig(comptime required_zig: []const u8) void {
    const current_vsn = builtin.zig_version;
    const required_vsn = std.SemanticVersion.parse(required_zig) catch
        @compileError("requireZig: invalid version string: " ++ required_zig);
    if (current_vsn.major != required_vsn.major or
        current_vsn.minor != required_vsn.minor or
        current_vsn.patch < required_vsn.patch or
        // ANY pre-release/dev toolchain (0.16.0-dev.N, 0.16.1-dev.N, ...) is
        // drift from the pinned RELEASE — reject all of them, not just the
        // pinned patch. The project pins an exact mise release; a dev build of
        // any patch is unshipped and unsupported here.
        current_vsn.pre != null)
    {
        @compileError(std.fmt.comptimePrint(
            "Your Zig version v{f} does not meet the required build version of v{f}",
            .{ current_vsn, required_vsn },
        ));
    }
}
