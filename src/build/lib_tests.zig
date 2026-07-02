//! The shared `src/lib` test step (`zig build test-lib`, the
//! `test-unit-agentsfleet-lib` make target) — one step, three compilations.
//!
//! `agentsfleet-lib-tests` file-imports contract + common so their tests
//! collect into its root module (the test runner only collects root-module
//! tests). A lib module that itself consumes NAMED modules cannot join that
//! root — one file cannot belong to two modules — so it roots its own
//! compilation in the exact module shape the production graphs use:
//! logging (→ common) and call_deadline (→ common + log).

const std = @import("std");
const shared = @import("shared.zig");

const S_COMMON = "common";
const S_LOG = "log";

pub fn addTestStep(
    b: *std.Build,
    target: std.Build.ResolvedTarget,
    optimize: std.builtin.OptimizeMode,
    test_filters: []const []const u8,
    deps: shared.SharedDeps,
) void {
    const lib_tests = b.addTest(.{
        .name = "agentsfleet-lib-tests",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/lib/tests.zig"),
            .target = target,
            .optimize = optimize,
        }),
        .filters = test_filters,
    });
    const logging_tests = b.addTest(.{
        .name = "agentsfleet-logging-tests",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/lib/logging/mod.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = S_COMMON, .module = deps.common },
            },
        }),
        .filters = test_filters,
    });
    const call_deadline_tests = b.addTest(.{
        .name = "agentsfleet-call-deadline-tests",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/lib/call_deadline/call_deadline.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = S_COMMON, .module = deps.common },
                .{ .name = S_LOG, .module = deps.log },
            },
        }),
        .filters = test_filters,
    });
    const lib_test_step = b.step("test-lib", "Run shared src/lib module unit tests");
    lib_test_step.dependOn(&b.addRunArtifact(lib_tests).step);
    lib_test_step.dependOn(&b.addRunArtifact(logging_tests).step);
    lib_test_step.dependOn(&b.addRunArtifact(call_deadline_tests).step);
}
