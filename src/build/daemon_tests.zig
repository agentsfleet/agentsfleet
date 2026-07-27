//! Daemon unit and live-service integration test graphs.

const std = @import("std");
const fixtures = @import("fixtures.zig");
const test_list = @import("test_list.zig");

const S_UNIT_NAME = "agentsfleetd-tests";
const S_UNIT_ROOT = "src/agentsfleetd/tests.zig";
const S_INTEGRATION_NAME = "agentsfleetd-integration-tests";
const S_INTEGRATION_ROOT = "src/agentsfleetd/integration_tests.zig";
const S_INTEGRATION_FILE_FILTER = "_integration_test";
const S_INTEGRATION_NAME_FILTER = "integration:";
const S_ROOT_DIR = "src/agentsfleetd";
const S_SHARD_RUNNER = "src/build/test_runner_shard.zig";

pub fn addTestSteps(
    b: *std.Build,
    target: std.Build.ResolvedTarget,
    optimize: std.builtin.OptimizeMode,
    filters: []const []const u8,
    imports: []const std.Build.Module.Import,
    list_step: *std.Build.Step,
) void {
    // The unit binary carries no datastore, so it is the one that can be run by
    // several processes at once — and it is the one that pays: the same artifact
    // is executed three times per change (plain, under kcov, under Valgrind).
    // The shard runner reproduces the default runner's leak, failure and
    // logged-error accounting exactly; with no shard environment set it runs
    // every test, so `zig build test` is unchanged in what it proves.
    //
    // Deliberately NOT applied to the integration binary below: sharding that
    // one needs per-shard datastores, which Redis cannot provide through
    // numbered databases (FLUSHALL and Pub/Sub are both global).
    const unit_tests = addTest(b, S_UNIT_NAME, S_UNIT_ROOT, target, optimize, filters, imports);
    unit_tests.test_runner = .{ .path = b.path(S_SHARD_RUNNER), .mode = .simple };
    fixtures.addDaemon(b, unit_tests.root_module);
    b.step("test", "Run agentsfleetd unit tests").dependOn(&b.addRunArtifact(unit_tests).step);
    installTest(b, "test-bin", "Install the agentsfleetd unit test binary", unit_tests);
    test_list.addLane(b, list_step, S_UNIT_NAME, unit_tests.root_module, S_ROOT_DIR);

    const integration_filters: []const []const u8 = if (filters.len == 0)
        &.{ S_INTEGRATION_FILE_FILTER, S_INTEGRATION_NAME_FILTER }
    else
        filters;
    const integration_tests = addTest(
        b,
        S_INTEGRATION_NAME,
        S_INTEGRATION_ROOT,
        target,
        optimize,
        integration_filters,
        imports,
    );
    fixtures.addDaemon(b, integration_tests.root_module);
    b.step("test-integration", "Run agentsfleetd live-service integration tests")
        .dependOn(&b.addRunArtifact(integration_tests).step);
    installTest(
        b,
        "test-integration-bin",
        "Install the agentsfleetd integration test binary",
        integration_tests,
    );
    test_list.addLane(b, list_step, S_INTEGRATION_NAME, integration_tests.root_module, S_ROOT_DIR);
}

fn addTest(
    b: *std.Build,
    name: []const u8,
    root: []const u8,
    target: std.Build.ResolvedTarget,
    optimize: std.builtin.OptimizeMode,
    filters: []const []const u8,
    imports: []const std.Build.Module.Import,
) *std.Build.Step.Compile {
    return b.addTest(.{
        .name = name,
        .root_module = b.createModule(.{
            .root_source_file = b.path(root),
            .target = target,
            .optimize = optimize,
            .imports = imports,
        }),
        .filters = filters,
    });
}

fn installTest(
    b: *std.Build,
    step_name: []const u8,
    description: []const u8,
    artifact: *std.Build.Step.Compile,
) void {
    b.step(step_name, description).dependOn(&b.addInstallArtifact(artifact, .{}).step);
}
