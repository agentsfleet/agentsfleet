const std = @import("std");
const builtin = @import("builtin");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});
    const with_bench_tools = b.option(bool, "with-bench-tools", "Enable benchmark tooling (zBench)") orelse false;
    const test_filter = b.option([]const u8, "test-filter", "Restrict Zig tests to names containing this substring");
    const git_commit = b.option([]const u8, "git-commit", "Git commit SHA embedded in the binary (passed from CI via GITHUB_SHA)") orelse "unknown";
    const build_opts = b.addOptions();
    build_opts.addOption([]const u8, "git_commit", git_commit);
    const test_filters: []const []const u8 = if (test_filter) |filter| &.{filter} else &.{};

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

    // ── httpz (pure-Zig HTTP server, karlseguin) ─────────────────────────────
    const httpz_dep = b.dependency("httpz", .{
        .target = target,
        .optimize = optimize,
    });
    const httpz_mod = httpz_dep.module("httpz");

    // ── pg.zig (pure-Zig Postgres driver) ────────────────────────────────────
    // OpenSSL is required for TLS connections to PlanetScale / hosted Postgres.
    // Only enabled when the target is Linux (our deployment platform) and the
    // host can provide headers. macOS cross-compile from Linux skips OpenSSL
    // (no macOS OpenSSL headers on Linux runners). macOS native dev uses Homebrew.
    const target_os = target.result.os.tag;
    const target_arch = target.result.cpu.arch;
    const host_is_linux = builtin.os.tag == .linux;
    const host_is_darwin = builtin.os.tag == .macos;
    const same_arch = builtin.cpu.arch == target_arch;
    // Enable OpenSSL when headers AND matching-arch libs are available:
    //   Linux host + Linux target + same arch: CI deploy build (system libssl-dev)
    //   macOS host + macOS target: local dev (Homebrew OpenSSL, native only)
    // Skip for cross-arch (e.g. x86_64 host → aarch64 target: libssl-dev
    // provides x86_64 libs, can't link into aarch64 binary) and cross-OS.
    const enable_openssl = (host_is_linux and target_os == .linux and same_arch) or (host_is_darwin and target_os == .macos);

    const pg_dep = if (enable_openssl) blk: {
        // Homebrew installs to /opt/homebrew on Apple Silicon, /usr/local on Intel.
        const homebrew_prefix = if (builtin.cpu.arch == .aarch64) "/opt/homebrew" else "/usr/local";
        const ssl_include: std.Build.LazyPath = .{ .cwd_relative = if (host_is_linux)
            "/usr/include"
        else
            homebrew_prefix ++ "/opt/openssl@3/include" };
        const ssl_lib: std.Build.LazyPath = .{ .cwd_relative = if (host_is_linux)
            b.fmt("/usr/lib/{s}", .{@tagName(builtin.cpu.arch) ++ "-linux-gnu"})
        else
            homebrew_prefix ++ "/opt/openssl@3/lib" };
        break :blk b.dependency("pg", .{
            .target = target,
            .optimize = optimize,
            .openssl_include_path = ssl_include,
            .openssl_lib_path = ssl_lib,
        });
    } else b.dependency("pg", .{
        .target = target,
        .optimize = optimize,
    });
    const pg_mod = pg_dep.module("pg");

    // Ubuntu/Debian multiarch: opensslconf.h lives in /usr/include/{arch}-linux-gnu/
    // rather than /usr/include/. Add the arch-specific include path so @cImport finds it.
    if (enable_openssl and host_is_linux) {
        pg_mod.addIncludePath(.{
            .cwd_relative = b.fmt("/usr/include/{s}-linux-gnu", .{@tagName(builtin.cpu.arch)}),
        });
    }

    // ── posthog-zig (server-side PostHog SDK) ───────────────────────────────
    const posthog_dep = b.dependency("posthog", .{
        .target = target,
        .optimize = optimize,
    });
    const posthog_mod = posthog_dep.module("posthog");

    // ── Schema embed module (root = schema/ so @embedFile is in-bounds) ──────
    const schema_mod = b.createModule(.{
        .root_source_file = b.path("schema/embed.zig"),
    });

    // ── Crypto primitives module (M28_001 §crypto — shared HMAC/CT/hex) ──────
    // Pure stdlib only; no deps. Importable from src/auth/ without breaking the
    // test-auth portability gate, and from src/zombie/ as the canonical source
    // for webhook signature verification primitives.
    const hmac_sig_mod = b.createModule(.{
        .root_source_file = b.path("src/crypto/hmac_sig.zig"),
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
                .{ .name = "httpz", .module = httpz_mod },
                .{ .name = "pg", .module = pg_mod },
                .{ .name = "posthog", .module = posthog_mod },
                .{ .name = "schema", .module = schema_mod },
                .{ .name = "build_options", .module = build_opts.createModule() },
                .{ .name = "hmac_sig", .module = hmac_sig_mod },
            },
        }),
    });

    // Only strip in ReleaseSmall (musl/minimal builds). ReleaseSafe keeps debug
    // info so panics produce usable stack traces in production.
    if (optimize == .ReleaseSmall) {
        exe.root_module.strip = true;
    }

    b.installArtifact(exe);

    // ── Sandbox executor sidecar ─────────────────────────────────────────────
    // Separate binary that serves the executor API over a Unix socket.
    // Embeds NullClaw and owns host-level Linux sandboxing.
    //
    // Built twice with different `executor_harness` build options:
    //   - `zombied-executor` (production): harness=false. Real LLM agent loop.
    //   - `zombied-executor-harness` (test fixture): harness=true. Comptime
    //     branch in runner.zig dispatches to runner_harness, which emits a
    //     scripted sequence of progress frames per the EXECUTOR_HARNESS_SCRIPT
    //     env var. Used by integration tests that need deterministic frame
    //     emission without spending tokens on a real LLM.
    //
    // Comptime gating means harness code is stripped from the production
    // binary — `if (build_options.executor_harness)` is dead code under
    // false. Verified via `cargo nm` / `objdump --syms` if paranoid; trust
    // the optimizer for routine cases.

    const exec_opts_prod = b.addOptions();
    exec_opts_prod.addOption(bool, "executor_harness", false);

    const exec_opts_harness = b.addOptions();
    exec_opts_harness.addOption(bool, "executor_harness", true);

    const executor_exe = b.addExecutable(.{
        .name = "zombied-executor",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/executor/main.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = "nullclaw", .module = nullclaw_mod },
                .{ .name = "build_options", .module = exec_opts_prod.createModule() },
            },
        }),
    });

    if (optimize == .ReleaseSmall) {
        executor_exe.root_module.strip = true;
    }

    b.installArtifact(executor_exe);

    const executor_harness_exe = b.addExecutable(.{
        .name = "zombied-executor-harness",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/executor/main.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = "nullclaw", .module = nullclaw_mod },
                .{ .name = "build_options", .module = exec_opts_harness.createModule() },
            },
        }),
    });

    b.installArtifact(executor_harness_exe);

    // ── Executor test step ───────────────────────────────────────────────────
    const executor_tests = b.addTest(.{
        .name = "zombied-executor-tests",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/executor/main.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = "nullclaw", .module = nullclaw_mod },
                .{ .name = "build_options", .module = exec_opts_prod.createModule() },
            },
        }),
    });
    b.step("test-executor", "Run executor unit tests").dependOn(&b.addRunArtifact(executor_tests).step);

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
                .{ .name = "httpz", .module = httpz_mod },
                .{ .name = "pg", .module = pg_mod },
                .{ .name = "posthog", .module = posthog_mod },
                .{ .name = "schema", .module = schema_mod },
                .{ .name = "build_options", .module = build_opts.createModule() },
                .{ .name = "hmac_sig", .module = hmac_sig_mod },
            },
        }),
        .filters = test_filters,
    });
    b.step("test", "Run unit tests").dependOn(&b.addRunArtifact(tests).step);

    // ── test-auth (M18_002 §1.3) ─────────────────────────────────────────────
    // Links ONLY src/auth/** and proves the portability contract: every module
    // under src/auth/ compiles in isolation without the rest of the project.
    // Any import that escapes the folder (directly or transitively) fails the
    // link here — so src/auth/ stays extractable into a standalone zombie-auth.
    const test_auth = b.addTest(.{
        .name = "zombied-test-auth",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/auth/tests.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = "httpz", .module = httpz_mod },
                .{ .name = "hmac_sig", .module = hmac_sig_mod },
            },
        }),
        .filters = test_filters,
    });
    b.step("test-auth", "Run src/auth/** tests in isolation (portability gate)")
        .dependOn(&b.addRunArtifact(test_auth).step);

    if (with_bench_tools) {
        // ── zBench dependency ────────────────────────────────────────────────
        const zbench_dep = b.dependency("zbench", .{
            .target = target,
            .optimize = optimize,
        });
        const zbench_mod = zbench_dep.module("zbench");

        // ── Tier-1 micro-benchmark runner (zBench-backed) ────────────────────
        // HTTP loadgen is handled by `hey` in make/test-bench.mk; see M24_001.
        const zbench_micro = b.addExecutable(.{
            .name = "zbench-micro",
            .root_module = b.createModule(.{
                .root_source_file = b.path("src/zbench_micro.zig"),
                .target = target,
                .optimize = optimize,
                .imports = &.{
                    .{ .name = "zbench", .module = zbench_mod },
                    .{ .name = "hmac_sig", .module = hmac_sig_mod },
                },
            }),
        });

        const run_zbench_micro = b.addRunArtifact(zbench_micro);
        if (b.args) |args| run_zbench_micro.addArgs(args);
        b.step("bench-micro", "Run Tier-1 zbench micro-benchmarks").dependOn(&run_zbench_micro.step);
    }

    // Installable backend test binary for coverage tooling (kcov/codecov).
    const install_tests = b.addInstallArtifact(tests, .{
        .dest_sub_path = "zombied-tests",
    });
    b.step("test-bin", "Build/install backend test binary for coverage").dependOn(&install_tests.step);
}
