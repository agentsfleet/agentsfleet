const std = @import("std");
const build_pg = @import("build_pg.zig");

const S_POSTHOG = "posthog";
const S_ZBENCH = "zbench";
const S_BUILD_OPTIONS = "build_options";
const S_SCHEMA = "schema";
const S_SRC_MAIN_ZIG = "src/zombied/main.zig";
const S_ZOMBIED_TESTS_ROOT = "src/zombied/tests.zig";
const S_NULLCLAW = "nullclaw";
const S_ZOMBIED_TESTS = "zombied-tests";
const S_LOG = "log";
const S_HMAC_SIG = "hmac_sig";
const S_AUTH_CODES = "auth_codes";
const S_PG = "pg";
const S_YAML = "yaml";
const S_CONTRACT = "contract";
const S_COMMON = "common";

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
    // need chat channels — usezombie runs agents programmatically).
    const nullclaw_dep = b.dependency(S_NULLCLAW, .{
        .target = target,
        .optimize = optimize,
        .channels = @as([]const u8, "none"),
        .engines = @as([]const u8, "base,sqlite"),
    });
    const nullclaw_mod = nullclaw_dep.module(S_NULLCLAW);

    // ── httpz (pure-Zig HTTP server, karlseguin) ─────────────────────────────
    const httpz_dep = b.dependency("httpz", .{
        .target = target,
        .optimize = optimize,
    });
    const httpz_mod = httpz_dep.module("httpz");

    const pg_mod = build_pg.module(b, target, optimize, S_PG);

    // ── posthog-zig (server-side PostHog SDK) ───────────────────────────────
    const posthog_dep = b.dependency(S_POSTHOG, .{
        .target = target,
        .optimize = optimize,
    });
    const posthog_mod = posthog_dep.module(S_POSTHOG);

    // ── zig-yaml (TRIGGER.md / SKILL.md frontmatter parsing) ────────────────
    // Pinned to 0.2.0 (Zig 0.15.x compatible). main targets Zig 0.16; do not
    // re-pin without verifying the toolchain. Replaces the bespoke YAML→JSON
    // converter in src/zombie/yaml_frontmatter.zig — gains depth-N nesting,
    // duplicate-key detection, and proper YAML 1.2 scalar handling.
    const zig_yaml_dep = b.dependency("zig_yaml", .{
        .target = target,
        .optimize = optimize,
    });
    const yaml_mod = zig_yaml_dep.module(S_YAML);

    // ── Schema embed module (root = schema/ so @embedFile is in-bounds) ──────
    const schema_mod = b.createModule(.{
        .root_source_file = b.path("schema/embed.zig"),
    });

    // ── Crypto primitives module: shared HMAC/CT/hex ─────────────────────────
    // Pure stdlib only; no deps. Importable from src/auth/ without breaking the
    // test-auth portability gate, and from src/zombie/ as the canonical source
    // for webhook signature verification primitives.
    const hmac_sig_mod = b.createModule(.{
        .root_source_file = b.path("src/zombied/crypto/hmac_sig.zig"),
    });

    // Auth-plane error-code mirror leaf — see auth_codes.zig header.
    const auth_codes_mod = b.createModule(.{
        .root_source_file = b.path("src/zombied/errors/auth_codes.zig"),
    });

    // ── Logging module ───────────────────────────────────────────────────────
    // Shared `log.scoped` API + pretty-printer + fatalStderr per
    // docs/LOGGING_STANDARD.md. Importable from every binary AND from
    // src/auth/ + the runner engine (which would otherwise be portability
    // islands forbidden from reaching across `src/`). Module-named import
    // makes the boundary clean — those layers still cannot import
    // arbitrary cross-layer code, just the canonical logging surface.
    //
    // Lives at src/logging/ — a peer of src/observability/ — because it's
    // strictly the structured-log facility. Wider observability concerns
    // (OTel exporters, metrics, traces) live under src/observability/ and
    // import this module.
    //
    // No domain dependencies (no error_registry import). Callers that
    // need to embed an error_code field in a log record pass it as a
    // struct field (`.{ .error_code = error_codes.ERR_X, ... }`), keeping
    // logging/ pure of business knowledge.
    const log_mod = b.createModule(.{
        .root_source_file = b.path("src/lib/logging/mod.zig"),
    });

    // Shared `/v1/runners` wire contract (src/lib/contract). A named module so
    // both build graphs reach it without crossing module boundaries (see
    // docs/ZIG_RULES.md "Module Boundaries & Shared Modules"). No deps — its
    // files import only std + each other within src/lib/contract/.
    const contract_mod = b.createModule(.{
        .root_source_file = b.path("src/lib/contract/contract.zig"),
    });

    // Single-source lease/runner knobs (src/lib/common) the control plane (fleet)
    // and the runner daemon both key off (RULE UFS). Named module: src/lib sits
    // outside the zombied module root, so it cannot be relative-imported.
    const common_mod = b.createModule(.{
        .root_source_file = b.path("src/lib/common/constants.zig"),
    });

    // Logging sources its envelope wall-clock from `common.clock` (Zig 0.16
    // removed std.time.*Timestamp). The log module is otherwise dependency-free;
    // `common` is a pure, datastore-free shared module, so this adds no domain
    // coupling and no cycle (common never imports log).
    log_mod.addImport(S_COMMON, common_mod);

    // hmac_sig sources its wall-clock from `common.clock` (Zig 0.16 removed
    // std.time.*Timestamp). Same pure, datastore-free shared module as log_mod —
    // no domain coupling, no cycle (common never imports hmac_sig).
    hmac_sig_mod.addImport(S_COMMON, common_mod);

    // ── usezombie executable ───────────────────────────────────────────────────
    const exe = b.addExecutable(.{
        .name = "zombied",
        .root_module = b.createModule(.{
            .root_source_file = b.path(S_SRC_MAIN_ZIG),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = S_NULLCLAW, .module = nullclaw_mod },
                .{ .name = "httpz", .module = httpz_mod },
                .{ .name = S_PG, .module = pg_mod },
                .{ .name = S_POSTHOG, .module = posthog_mod },
                .{ .name = S_SCHEMA, .module = schema_mod },
                .{ .name = S_BUILD_OPTIONS, .module = build_opts.createModule() },
                .{ .name = S_HMAC_SIG, .module = hmac_sig_mod },
                .{ .name = S_AUTH_CODES, .module = auth_codes_mod },
                .{ .name = S_LOG, .module = log_mod },
                .{ .name = S_CONTRACT, .module = contract_mod },
                .{ .name = S_COMMON, .module = common_mod },
                .{ .name = S_YAML, .module = yaml_mod },
            },
        }),
    });

    // Only strip in ReleaseSmall (musl/minimal builds). ReleaseSafe keeps debug
    // info so panics produce usable stack traces in production.
    if (optimize == .ReleaseSmall) {
        exe.root_module.strip = true;
    }

    b.installArtifact(exe);

    // Execution left this build graph at the M80 cutover: the standalone sandbox
    // sidecar (and its harness/stub fixtures) is gone, replaced
    // by the host-resident `zombie-runner` daemon, which has its own build graph
    // (`build_runner.zig`) and never links zombied's server infrastructure
    // (pg/httpz/redis). It shares only the frozen wire protocol by source.

    // ── Shared src/lib test step ─────────────────────────────────────────────
    // One pass over every shared module under src/lib (each is a named module
    // reused across build graphs); their own tests run here, in each module's
    // own instance, so they reach internals consumers never see. The aggregator
    // (src/lib/tests.zig) grows by one line per new src/lib module.
    const lib_tests = b.addTest(.{
        .name = "zombie-lib-tests",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/lib/tests.zig"),
            .target = target,
            .optimize = optimize,
        }),
        .filters = test_filters,
    });
    b.step("test-lib", "Run shared src/lib module unit tests").dependOn(&b.addRunArtifact(lib_tests).step);

    // ── Run step ─────────────────────────────────────────────────────────────
    const run_cmd = b.addRunArtifact(exe);
    run_cmd.step.dependOn(b.getInstallStep());
    if (b.args) |args| run_cmd.addArgs(args);
    b.step("run", "Run zombied").dependOn(&run_cmd.step);

    // ── Test step ─────────────────────────────────────────────────────────────
    const tests = b.addTest(.{
        .name = S_ZOMBIED_TESTS,
        .root_module = b.createModule(.{
            .root_source_file = b.path(S_ZOMBIED_TESTS_ROOT),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = S_NULLCLAW, .module = nullclaw_mod },
                .{ .name = "httpz", .module = httpz_mod },
                .{ .name = S_PG, .module = pg_mod },
                .{ .name = S_POSTHOG, .module = posthog_mod },
                .{ .name = S_SCHEMA, .module = schema_mod },
                .{ .name = S_BUILD_OPTIONS, .module = build_opts.createModule() },
                .{ .name = S_HMAC_SIG, .module = hmac_sig_mod },
                .{ .name = S_AUTH_CODES, .module = auth_codes_mod },
                .{ .name = S_LOG, .module = log_mod },
                .{ .name = S_CONTRACT, .module = contract_mod },
                .{ .name = S_COMMON, .module = common_mod },
                .{ .name = S_YAML, .module = yaml_mod },
            },
        }),
        .filters = test_filters,
    });
    const run_tests = b.addRunArtifact(tests);
    b.step("test", "Run unit tests").dependOn(&run_tests.step);

    // ── test-auth ────────────────────────────────────────────────────────────
    // Links ONLY src/auth/** and proves the portability contract: every module
    // under src/auth/ compiles in isolation without the rest of the project.
    // Any import that escapes the folder (directly or transitively) fails the
    // link here — so src/auth/ stays extractable into a standalone zombie-auth.
    const test_auth = b.addTest(.{
        .name = "zombied-test-auth",
        .root_module = b.createModule(.{
            .root_source_file = b.path("src/zombied/auth/tests.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = "httpz", .module = httpz_mod },
                .{ .name = S_HMAC_SIG, .module = hmac_sig_mod },
                .{ .name = S_AUTH_CODES, .module = auth_codes_mod },
                .{ .name = S_LOG, .module = log_mod },
                .{ .name = S_CONTRACT, .module = contract_mod },
                .{ .name = S_COMMON, .module = common_mod },
            },
        }),
        .filters = test_filters,
    });
    // src/auth/ now imports the named "log" module so it can use obs.scoped
    // without breaking the portability gate (the gate forbids reaching into
    // src/observability/ via relative paths, but named modules are
    // first-class deps that don't violate the layer boundary).
    b.step("test-auth", "Run src/auth/** tests in isolation (portability gate)")
        .dependOn(&b.addRunArtifact(test_auth).step);

    if (with_bench_tools) {
        // ── zBench dependency ────────────────────────────────────────────────
        const zbench_dep = b.dependency(S_ZBENCH, .{
            .target = target,
            .optimize = optimize,
        });
        const zbench_mod = zbench_dep.module(S_ZBENCH);

        // ── bench bridge module ──────────────────────────────────────────────
        // Re-exports `src/` internals so bench exes under `tests/bench/` can
        // reach them. Rooted at `src/bench_exports.zig` (inside src/) so the
        // module-root walk stays legal under Zig 0.15.2's strict boundaries.
        const bench_app_mod = b.createModule(.{
            .root_source_file = b.path("src/zombied/bench_exports.zig"),
            .target = target,
            .optimize = optimize,
            .imports = &.{
                .{ .name = "httpz", .module = httpz_mod },
                .{ .name = S_HMAC_SIG, .module = hmac_sig_mod },
                .{ .name = S_LOG, .module = log_mod },
                .{ .name = S_CONTRACT, .module = contract_mod },
                .{ .name = S_COMMON, .module = common_mod },
                .{ .name = S_AUTH_CODES, .module = auth_codes_mod },
            },
        });

        // ── Tier-1 micro-benchmark runner (zBench-backed) ────────────────────
        // HTTP loadgen is handled by `hey` in make/bench.mk.
        const bench_micro = b.addExecutable(.{
            .name = "bench-micro",
            .root_module = b.createModule(.{
                .root_source_file = b.path("tests/bench/micro.zig"),
                .target = target,
                .optimize = optimize,
                .imports = &.{
                    .{ .name = S_ZBENCH, .module = zbench_mod },
                    .{ .name = "bench_app", .module = bench_app_mod },
                },
            }),
        });

        const run_bench_micro = b.addRunArtifact(bench_micro);
        if (b.args) |args| run_bench_micro.addArgs(args);
        b.step("bench-micro", "Run Tier-1 zbench micro-benchmarks").dependOn(&run_bench_micro.step);

        // ── Redis XADD concurrency bench ─────────────────────────────────────
        // 8 producer threads × 1000 XADDs against a live Redis. Skip-by-default
        // unless BENCH_REDIS=1 — see tests/bench/redis_xadd_concurrency.zig.
        const bench_redis = b.addExecutable(.{
            .name = "bench-redis",
            .root_module = b.createModule(.{
                .root_source_file = b.path("tests/bench/redis_xadd_concurrency.zig"),
                .target = target,
                .optimize = optimize,
                .imports = &.{
                    .{ .name = "bench_app", .module = bench_app_mod },
                },
            }),
        });

        const run_bench_redis = b.addRunArtifact(bench_redis);
        if (b.args) |args| run_bench_redis.addArgs(args);
        b.step("bench-redis", "Run Redis XADD concurrency bench (BENCH_REDIS=1)").dependOn(&run_bench_redis.step);
    }

    // Installable backend test binary for coverage tooling (kcov/codecov).
    const install_tests = b.addInstallArtifact(tests, .{
        .dest_sub_path = S_ZOMBIED_TESTS,
    });
    b.step("test-bin", "Build/install backend test binary for coverage").dependOn(&install_tests.step);
}
