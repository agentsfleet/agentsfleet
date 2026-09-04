const std = @import("std");
const buildpkg = @import("src/build/main.zig");

comptime {
    // Fail fast (with a clear message) if the toolchain drifted from the
    // minimum_zig_version pinned in build.zig.zon.
    buildpkg.requireZig(@import("build.zig.zon").minimum_zig_version);
}

/// What this graph builds, now that the daemon is Rust.
///
/// The Zig daemon tree was this file's reason to exist: an
/// executable, a `run` step, the `error-codes.mdx` generator, the `test-auth`
/// portability gate, the daemon test lanes and the zBench bridge. All of it
/// went with the tree, and with it every module those targets were the only
/// consumer of — httpz, pg, cache, posthog, nullclaw, schema, yaml, s3 and the
/// hmac/auth-code leaves.
///
/// What remains is what was never the daemon's: the shared `src/lib` tests,
/// which `agentsfleet-runner` links by source and which therefore outlive the
/// binary they were extracted from, plus the `test-s3` build-wiring gate and
/// the incident-bench steps. The runner keeps its own graph in
/// `build_runner.zig` and is untouched by any of this.
pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});
    const test_filter = b.option([]const u8, "test-filter", "Restrict Zig tests to names containing this substring");
    const test_filters: []const []const u8 = if (test_filter) |filter| &.{filter} else &.{};

    const deps = buildpkg.shared.SharedDeps.init(b, target, optimize);

    // `list-tests`: one list-only compilation per test binary, printing the
    // tests the compiler actually registered. Created before the steps that
    // attach lanes to it.
    const list_step = b.step(buildpkg.test_list.STEP_NAME, buildpkg.test_list.STEP_DESC);

    // ── Shared src/lib test step (`test-lib`) ────────────────────────────────
    // One step covering the src/lib barrel plus the named-module-consuming lib
    // modules (logging, call_deadline), each compiled in its production shape.
    buildpkg.lib_tests.addTestStep(b, target, optimize, test_filters, deps, list_step);

    buildpkg.bench_incident.addSteps(b, target, optimize, test_filters);
}
