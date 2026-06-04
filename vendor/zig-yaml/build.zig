const std = @import("std");
// Vendor patch — the upstream YAML Test Suite harness (test/spec.zig) uses
// std APIs removed in Zig 0.16 (std.StringArrayHashMap, std.fs.cwd reshape).
// Because build.zig imported it at top level and referenced SpecTest.create in
// an analyzed branch, the broken harness failed every consumer's build. We only
// consume the `yaml` module, so the conformance step is dropped here. See
// vendor/zig-yaml/CHANGES.md.

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const enable_logging = b.option(bool, "log", "Whether to enable logging") orelse false;
    const yaml_module = b.addModule("yaml", .{
        .root_source_file = b.path("src/lib.zig"),
        .target = target,
        .optimize = optimize,
    });

    const yaml_tests = b.addTest(.{
        .root_module = yaml_module,
    });

    const cli = b.addModule("cli", .{
        .root_source_file = b.path("examples/yaml.zig"),
        .target = target,
        .optimize = optimize,
    });
    const example = b.addExecutable(.{
        .name = "yaml",
        .root_module = cli,
    });
    example.root_module.addImport("yaml", yaml_module);

    const example_opts = b.addOptions();
    example.root_module.addOptions("build_options", example_opts);
    example_opts.addOption(bool, "enable_logging", enable_logging);

    b.installArtifact(example);

    const run_cmd = b.addRunArtifact(example);
    run_cmd.step.dependOn(b.getInstallStep());
    if (b.args) |args| {
        run_cmd.addArgs(args);
    }

    const run_step = b.step("run", "Run example program parser");
    run_step.dependOn(&run_cmd.step);

    const test_step = b.step("test", "Run library tests");
    test_step.dependOn(&b.addRunArtifact(yaml_tests).step);

    const e2e_tests_module = b.addModule("test", .{
        .root_source_file = b.path("test/test.zig"),
        .target = target,
        .optimize = optimize,
    });
    var e2e_tests = b.addTest(.{
        .root_module = e2e_tests_module,
    });
    e2e_tests.root_module.addImport("yaml", yaml_module);
    test_step.dependOn(&b.addRunArtifact(e2e_tests).step);

    // Vendor patch — YAML Test Suite conformance step dropped (see top of file).
}
