const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    // ── NullClaw dependency ──────────────────────────────────────────────────
    // Use base engines (sqlite for per-run memory) + no channels (we don't
    // need chat channels — UseZombie runs agents programmatically).
    const nullclaw_dep = b.dependency("nullclaw", .{
        .target = target,
        .optimize = optimize,
        .channels = @as([]const u8, "none"),
        .engines = @as([]const u8, "base,sqlite"),
    });
    const nullclaw_mod = nullclaw_dep.module("nullclaw");

    // ── Zap HTTP dependency ──────────────────────────────────────────────────
    const zap_dep = b.dependency("zap", .{
        .target = target,
        .optimize = optimize,
        .openssl = false,
    });
    const zap_mod = zap_dep.module("zap");

    // ── pg.zig (pure-Zig Postgres driver) ────────────────────────────────────
    const pg_dep = b.dependency("pg", .{
        .target = target,
        .optimize = optimize,
    });
    const pg_mod = pg_dep.module("pg");

    // ── Schema embed module (root = schema/ so @embedFile is in-bounds) ──────
    const schema_mod = b.createModule(.{
        .root_source_file = b.path("schema/embed.zig"),
    });

    // ── UseZombie executable ───────────────────────────────────────────────────
    const exe = b.addExecutable(.{
        .name = "zombied",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/main.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = "nullclaw", .module = nullclaw_mod },
                .{ .name = "zap", .module = zap_mod },
                .{ .name = "pg", .module = pg_mod },
                .{ .name = "schema", .module = schema_mod },
            },
        }),
    });

    if (optimize != .Debug) {
        exe.root_module.strip = true;
    }

    b.installArtifact(exe);

    // ── Run step ─────────────────────────────────────────────────────────────
    const run_cmd = b.addRunArtifact(exe);
    run_cmd.step.dependOn(b.getInstallStep());
    if (b.args) |args| run_cmd.addArgs(args);
    b.step("run", "Run zombied").dependOn(&run_cmd.step);

    // ── Test step ─────────────────────────────────────────────────────────────
    const tests = b.addTest(.{
        .name = "zombied-tests",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/main.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = "nullclaw", .module = nullclaw_mod },
                .{ .name = "zap", .module = zap_mod },
                .{ .name = "pg", .module = pg_mod },
                .{ .name = "schema", .module = schema_mod },
            },
        }),
    });
    b.step("test", "Run unit tests").dependOn(&b.addRunArtifact(tests).step);

    // Installable backend test binary for coverage tooling (kcov/codecov).
    const install_tests = b.addInstallArtifact(tests, .{
        .dest_sub_path = "zombied-tests",
    });
    b.step("test-bin", "Build/install backend test binary for coverage").dependOn(&install_tests.step);
}
