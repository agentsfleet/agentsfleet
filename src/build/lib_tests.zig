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
const test_list = @import("test_list.zig");

const S_COMMON = "common";
const S_LOG = "log";

const S_LIB_TESTS = "agentsfleet-lib-tests";
const S_LOGGING_TESTS = "agentsfleet-logging-tests";
const S_CALL_DEADLINE_TESTS = "agentsfleet-call-deadline-tests";
const S_LIB_TESTS_ROOT = "src/lib/tests.zig";
const S_LOGGING_ROOT = "src/lib/logging/mod.zig";
const S_CALL_DEADLINE_ROOT = "src/lib/call_deadline/call_deadline.zig";
// Each compilation names its tests relative to its own root directory, so the
// three lanes below report three different namespaces for the same `src/lib` tree.
const S_LIB_ROOT_DIR = "src/lib";
const S_LOGGING_ROOT_DIR = "src/lib/logging";
const S_CALL_DEADLINE_ROOT_DIR = "src/lib/call_deadline";

pub fn addTestStep(
    b: *std.Build,
    target: std.Build.ResolvedTarget,
    optimize: std.builtin.OptimizeMode,
    test_filters: []const []const u8,
    deps: shared.SharedDeps,
    list_step: *std.Build.Step,
) void {
    const lib_tests = b.addTest(.{
        .name = S_LIB_TESTS,
        .root_module = b.createModule(.{
            .root_source_file = b.path(S_LIB_TESTS_ROOT),
            .target = target,
            .optimize = optimize,
        }),
        .filters = test_filters,
    });
    const logging_tests = b.addTest(.{
        .name = S_LOGGING_TESTS,
        .root_module = b.createModule(.{
            .root_source_file = b.path(S_LOGGING_ROOT),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = S_COMMON, .module = deps.common },
            },
        }),
        .filters = test_filters,
    });
    const call_deadline_tests = b.addTest(.{
        .name = S_CALL_DEADLINE_TESTS,
        .root_module = b.createModule(.{
            .root_source_file = b.path(S_CALL_DEADLINE_ROOT),
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

    test_list.addLane(b, list_step, S_LIB_TESTS, lib_tests.root_module, S_LIB_ROOT_DIR);
    test_list.addLane(b, list_step, S_LOGGING_TESTS, logging_tests.root_module, S_LOGGING_ROOT_DIR);
    test_list.addLane(b, list_step, S_CALL_DEADLINE_TESTS, call_deadline_tests.root_module, S_CALL_DEADLINE_ROOT_DIR);
}
