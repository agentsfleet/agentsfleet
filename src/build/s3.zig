//! Build helper for the Cloudflare R2 object-store wrapper — split out of
//! build.zig (RULE FLL), mirroring pg.zig.
//!
//! z3 (codeberg.org/fellowtraveler/z3) exposes its S3 client as the module
//! named "s3" (root src/s3_client.zig); src/lib/s3/r2.zig imports it under the
//! name `z3`. Wired into the DAEMON graph only — the runner holds zero
//! datastore credentials (build_runner.zig), so on import the daemon PUTs the
//! immutable raw repo tarball and at lease GETs it; bundle bytes reach the
//! sandbox via the lease, never a runner-side R2 get.

const std = @import("std");
const test_list = @import("test_list.zig");

const DEP_Z3 = "z3"; // build.zig.zon dependency key AND r2.zig's import name
const Z3_S3_MODULE = "s3"; // module name z3 exposes its S3 client under
const R2_ROOT = "src/lib/s3/r2.zig";
const S3_TESTS = "agentsfleet-s3-tests";
// r2.zig IS the root of its own test compilation, so its tests are namespaced
// against the directory it sits in, not against `src/lib`.
const R2_ROOT_DIR = "src/lib/s3";

/// The `s3` module: r2.zig with z3's S3 client wired in under the `z3` import.
pub fn module(
    b: *std.Build,
    target: std.Build.ResolvedTarget,
    optimize: std.builtin.OptimizeMode,
) *std.Build.Module {
    return b.createModule(.{
        .root_source_file = b.path(R2_ROOT),
        .imports = &.{
            .{ .name = DEP_Z3, .module = z3Module(b, target, optimize) },
        },
    });
}

/// Adds a `test-s3` step that compiles r2.zig against z3 standalone — a
/// verifiable build-wiring gate before any daemon consumer imports the module.
pub fn addTestStep(
    b: *std.Build,
    target: std.Build.ResolvedTarget,
    optimize: std.builtin.OptimizeMode,
    test_filters: []const []const u8,
    list_step: *std.Build.Step,
) void {
    const s3_tests = b.addTest(.{
        .name = S3_TESTS,
        .root_module = b.createModule(.{
            .root_source_file = b.path(R2_ROOT),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = DEP_Z3, .module = z3Module(b, target, optimize) },
            },
        }),
        .filters = test_filters,
    });
    b.step("test-s3", "Compile + test the R2/z3 wrapper standalone")
        .dependOn(&b.addRunArtifact(s3_tests).step);
    test_list.addLane(b, list_step, S3_TESTS, s3_tests.root_module, R2_ROOT_DIR);
}

fn z3Module(
    b: *std.Build,
    target: std.Build.ResolvedTarget,
    optimize: std.builtin.OptimizeMode,
) *std.Build.Module {
    const z3_dep = b.dependency(DEP_Z3, .{
        .target = target,
        .optimize = optimize,
    });
    return z3_dep.module(Z3_S3_MODULE);
}
