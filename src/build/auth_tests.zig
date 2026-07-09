//! The `zig build test-auth` step — the `src/agentsfleetd/auth/**` portability gate.
//! Extracted from build.zig (RULE FLL), mirroring lib_tests.zig and s3.zig.
//!
//! Links ONLY `src/agentsfleetd/auth/**` and proves the portability invariant: every
//! module under it compiles in isolation from the rest of the project. Any import
//! that escapes the folder, directly or transitively, fails the link here — so
//! `src/agentsfleetd/auth/` stays extractable into a standalone agentsfleet-auth.
//!
//! The folder does import the named `log` module for `obs.scoped`. Named modules are
//! first-class dependencies and do not violate the layer boundary; what the gate
//! forbids is reaching into `src/agentsfleetd/observability/` by relative path.

const std = @import("std");
const test_list = @import("test_list.zig");

const STEP_NAME = "test-auth";
const STEP_DESC = "Run src/agentsfleetd/auth/** tests in isolation (portability gate)";
const TEST_NAME = "agentsfleetd-test-auth";
const ROOT_SOURCE = "src/agentsfleetd/auth/tests.zig";
/// Zig namespaces each registered test against the root module's directory; the
/// reachability checker needs it to map a test name back to a file.
const ROOT_DIR = "src/agentsfleetd/auth";

pub fn addTestStep(
    b: *std.Build,
    target: std.Build.ResolvedTarget,
    optimize: std.builtin.OptimizeMode,
    test_filters: []const []const u8,
    imports: []const std.Build.Module.Import,
    list_step: *std.Build.Step,
) void {
    const test_auth = b.addTest(.{
        .name = TEST_NAME,
        .root_module = b.createModule(.{
            .root_source_file = b.path(ROOT_SOURCE),
            .target = target,
            .optimize = optimize,
            .imports = imports,
        }),
        .filters = test_filters,
    });
    b.step(STEP_NAME, STEP_DESC).dependOn(&b.addRunArtifact(test_auth).step);
    test_list.addLane(b, list_step, TEST_NAME, test_auth.root_module, ROOT_DIR);
}
