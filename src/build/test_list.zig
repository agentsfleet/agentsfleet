//! The `list-tests` lane — a second, list-only compilation of every test binary.
//!
//! Each lane shares its root module with the real `test` step but swaps in
//! `test_runner_list.zig`, which prints the registered test names instead of running
//! them. Sharing the module is what makes the listing trustworthy: it is the same
//! import graph the real suite compiles, so a block that registers here registers
//! there.
//!
//! The real `test` steps are deliberately left on Zig's default runner. Swapping
//! them would mean re-implementing the `std.zig.Server` stdio protocol that
//! `zig build test --summary all` reads its pass/skip accounting from.
//!
//! Lanes carry no `filters`: `-Dtest-filter` prunes `builtin.test_functions` at
//! compile time, and a filtered listing would report live blocks as dead.

const std = @import("std");

pub const STEP_NAME = "list-tests";
pub const STEP_DESC = "Print every compiler-registered test name (reachability gate)";

const RUNNER_PATH = "src/build/test_runner_list.zig";
const NAME_SUFFIX = "-list";

/// Attach a list-only compilation of `root_module` to `step`.
///
/// `root_dir` is the directory `root_module`'s root source file lives in. Zig names
/// each test after its source path relative to that directory, so the checker needs
/// it to map a registered name back to a file on disk.
pub fn addLane(
    b: *std.Build,
    step: *std.Build.Step,
    name: []const u8,
    root_module: *std.Build.Module,
    root_dir: []const u8,
) void {
    const listing = b.addTest(.{
        .name = b.fmt("{s}{s}", .{ name, NAME_SUFFIX }),
        .root_module = root_module,
        .test_runner = .{ .path = b.path(RUNNER_PATH), .mode = .simple },
    });
    const run = b.addRunArtifact(listing);
    run.addArg(root_dir);
    step.dependOn(&run.step);
}
