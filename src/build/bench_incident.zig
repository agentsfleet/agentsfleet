//! Build helper for the incident-response benchmark harness — split out of
//! build.zig (RULE FLL), mirroring s3.zig.
//!
//! Pure-std module rooted outside `src/`. It scores detection findings against
//! seed manifests, with no daemon internals in reach. Its tests ride their own
//! step because the reachability checker walks `src/` only.
//!
//! Deliberately outside build.zig's `with_bench_tools` gate. That gate is for
//! the zBench benchmarks; gating this harness once left its tests in no lane.

const std = @import("std");
const shared = @import("shared.zig");

const BENCH_ROOT = "bench/incident-response/main.zig";
const BENCH_NAME = "bench-incident";
const BENCH_TESTS_NAME = "bench-incident-tests";
const BENCH_STEP = "bench-incident";
const BENCH_STEP_DESC = "Run the incident-response benchmark harness";
const BENCH_TEST_STEP = "bench-incident-test";
const BENCH_TEST_STEP_DESC = "Run incident-response harness unit tests";

/// Registers the `bench-incident` run step and its `bench-incident-test` sibling.
/// Both share one module, so the harness and its tests cannot drift apart.
pub fn addSteps(
    b: *std.Build,
    target: std.Build.ResolvedTarget,
    optimize: std.builtin.OptimizeMode,
    test_filters: []const []const u8,
) void {
    const bench_mod = b.createModule(.{
        .root_source_file = b.path(BENCH_ROOT),
        .target = target,
        .optimize = optimize,
    });

    const bench = b.addExecutable(.{
        .name = BENCH_NAME,
        .root_module = bench_mod,
    });
    const run_bench = b.addRunArtifact(bench);
    if (b.args) |args| run_bench.addArgs(args);
    b.step(BENCH_STEP, BENCH_STEP_DESC).dependOn(&run_bench.step);

    const bench_tests = b.addTest(.{
        .use_llvm = shared.TEST_USE_LLVM,
        .name = BENCH_TESTS_NAME,
        .root_module = bench_mod,
        .filters = test_filters,
    });
    const run_bench_tests = b.addRunArtifact(bench_tests);
    b.step(BENCH_TEST_STEP, BENCH_TEST_STEP_DESC).dependOn(&run_bench_tests.step);
}
